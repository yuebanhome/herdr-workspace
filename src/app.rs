use std::io::{self, Stdout};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex, mpsc};
use std::thread;
use std::time::{Duration, Instant};

use anyhow::{Context, Result};
use crossterm::cursor::SetCursorStyle;
use crossterm::event::{
    self, DisableFocusChange, DisableMouseCapture, EnableFocusChange, EnableMouseCapture, Event,
    KeyCode, KeyEventKind, KeyModifiers, MouseButton, MouseEventKind,
};
use crossterm::execute;
use crossterm::terminal::{
    EnterAlternateScreen, LeaveAlternateScreen, SetTitle, disable_raw_mode, enable_raw_mode,
};
use ratatui::Terminal;
use ratatui::backend::CrosstermBackend;

use crate::discovery::discover_repositories;
use crate::git::{read_repository, sanitize};
use crate::model::Repository;
use crate::state::AppState;
use crate::ui;

const EVENT_TICK: Duration = Duration::from_millis(100);
const DISCOVERY_INTERVAL: Duration = Duration::from_secs(30);

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum ScanMode {
    Discover,
    Status,
}

struct WorkspaceSnapshot {
    repositories: Vec<Repository>,
    skipped_paths: usize,
}

struct ScanMessage {
    mode: ScanMode,
    result: std::result::Result<WorkspaceSnapshot, String>,
}

pub fn run(root: PathBuf) -> Result<()> {
    let mut terminal = TerminalGuard::new()?;
    let (sender, receiver) = mpsc::channel();
    let mut state = AppState::new(root);
    start_scan(&mut state, sender.clone(), ScanMode::Discover);
    let mut next_refresh = Instant::now() + refresh_interval(0);
    let mut next_discovery = Instant::now() + DISCOVERY_INTERVAL;

    loop {
        terminal
            .terminal
            .draw(|frame| ui::draw(frame, &mut state))?;

        if event::poll(EVENT_TICK)? {
            match event::read()? {
                Event::Key(key) if key.kind == KeyEventKind::Press => match key.code {
                    KeyCode::Char('q') | KeyCode::Esc => break,
                    KeyCode::Char('c') if key.modifiers.contains(KeyModifiers::CONTROL) => break,
                    KeyCode::Down | KeyCode::Char('j') => state.move_selection(1),
                    KeyCode::Up | KeyCode::Char('k') => state.move_selection(-1),
                    KeyCode::Home | KeyCode::Char('g') => state.select_first(),
                    KeyCode::End | KeyCode::Char('G') => state.select_last(),
                    KeyCode::Enter | KeyCode::Char(' ') => state.toggle_selected(),
                    KeyCode::Char('r') => request_discovery(&mut state, sender.clone()),
                    _ => {}
                },
                Event::Mouse(mouse) => match mouse.kind {
                    MouseEventKind::Down(MouseButton::Left) => state.click(mouse.row),
                    MouseEventKind::ScrollDown => state.move_selection(3),
                    MouseEventKind::ScrollUp => state.move_selection(-3),
                    _ => {}
                },
                Event::FocusGained => request_discovery(&mut state, sender.clone()),
                Event::Resize(_, _) => state.ensure_selected_visible(),
                _ => {}
            }
        }

        while let Ok(message) = receiver.try_recv() {
            state.scanning = false;
            match message.result {
                Ok(snapshot) => {
                    state.error = None;
                    if message.mode == ScanMode::Discover {
                        state.skipped_paths = snapshot.skipped_paths;
                        next_discovery = Instant::now() + DISCOVERY_INTERVAL;
                    }
                    state.replace_repositories(snapshot.repositories);
                }
                Err(error) => state.error = Some(error),
            }
            if state.scan_queued {
                state.scan_queued = false;
                start_scan(&mut state, sender.clone(), ScanMode::Discover);
            } else {
                next_refresh = Instant::now() + refresh_interval(state.repositories.len());
            }
        }

        let now = Instant::now();
        if let Some(mode) = scheduled_scan_mode(now, next_refresh, next_discovery, state.scanning) {
            start_scan(&mut state, sender.clone(), mode);
        }
    }
    Ok(())
}

fn request_discovery(state: &mut AppState, sender: mpsc::Sender<ScanMessage>) {
    if state.scanning {
        state.scan_queued = true;
    } else {
        start_scan(state, sender, ScanMode::Discover);
    }
}

fn start_scan(state: &mut AppState, sender: mpsc::Sender<ScanMessage>, mode: ScanMode) {
    state.scanning = true;
    let root = state.root.clone();
    let known_paths = state
        .repositories
        .iter()
        .map(|repository| repository.root.clone())
        .collect::<Vec<_>>();
    let mode = if mode == ScanMode::Status && known_paths.is_empty() {
        ScanMode::Discover
    } else {
        mode
    };
    thread::spawn(move || {
        let result = match mode {
            ScanMode::Discover => scan_workspace_snapshot(&root),
            ScanMode::Status => Ok(WorkspaceSnapshot {
                repositories: scan_repositories(&root, known_paths),
                skipped_paths: 0,
            }),
        }
        .map_err(bound_scan_error);
        let _ = sender.send(ScanMessage { mode, result });
    });
}

fn bound_scan_error(error: anyhow::Error) -> String {
    let sanitized = sanitize(&format!("{error:#}"));
    let mut bounded: String = sanitized.chars().take(160).collect();
    if sanitized.chars().count() > 160 {
        bounded.push_str("...");
    }
    bounded
}

pub fn scan_workspace(root: &Path) -> Result<Vec<Repository>> {
    Ok(scan_workspace_snapshot(root)?.repositories)
}

fn scan_workspace_snapshot(root: &Path) -> Result<WorkspaceSnapshot> {
    let discovery = discover_repositories(root)
        .with_context(|| format!("could not scan workspace {}", root.display()))?;
    Ok(WorkspaceSnapshot {
        repositories: scan_repositories(root, discovery.repositories),
        skipped_paths: discovery.skipped_paths,
    })
}

fn scan_repositories(root: &Path, paths: Vec<PathBuf>) -> Vec<Repository> {
    if paths.is_empty() {
        return Vec::new();
    }

    let paths = Arc::new(paths);
    let next = Arc::new(AtomicUsize::new(0));
    let repositories = Arc::new(Mutex::new(Vec::with_capacity(paths.len())));
    let worker_count = thread::available_parallelism()
        .map(usize::from)
        .unwrap_or(4)
        .clamp(1, 8)
        .min(paths.len());

    thread::scope(|scope| {
        for _ in 0..worker_count {
            let paths = Arc::clone(&paths);
            let next = Arc::clone(&next);
            let repositories = Arc::clone(&repositories);
            scope.spawn(move || {
                loop {
                    let index = next.fetch_add(1, Ordering::Relaxed);
                    let Some(path) = paths.get(index) else {
                        break;
                    };
                    repositories.lock().unwrap().push(read_repository(path));
                }
            });
        }
    });

    let mut repositories = Arc::try_unwrap(repositories)
        .expect("all scan workers have completed")
        .into_inner()
        .expect("repository result lock is not poisoned");
    disambiguate_names(root, &mut repositories);
    repositories.sort_by(|left, right| {
        left.name
            .to_lowercase()
            .cmp(&right.name.to_lowercase())
            .then_with(|| left.root.cmp(&right.root))
    });
    repositories
}

fn disambiguate_names(workspace: &Path, repositories: &mut [Repository]) {
    use std::collections::HashMap;

    let mut counts = HashMap::new();
    for repository in repositories.iter() {
        *counts
            .entry(repository.name.to_lowercase())
            .or_insert(0usize) += 1;
    }
    for repository in repositories {
        if counts
            .get(&repository.name.to_lowercase())
            .is_some_and(|count| *count > 1)
        {
            let relative = repository
                .root
                .strip_prefix(workspace)
                .unwrap_or(&repository.root);
            let label = sanitize(&relative.to_string_lossy());
            if !label.is_empty() {
                repository.name = label;
            }
        }
    }
}

fn refresh_interval(repository_count: usize) -> Duration {
    match repository_count {
        0..=25 => Duration::from_secs(2),
        26..=100 => Duration::from_secs(5),
        _ => Duration::from_secs(10),
    }
}

fn scheduled_scan_mode(
    now: Instant,
    next_refresh: Instant,
    next_discovery: Instant,
    scanning: bool,
) -> Option<ScanMode> {
    if scanning || now < next_refresh {
        return None;
    }
    Some(if now >= next_discovery {
        ScanMode::Discover
    } else {
        ScanMode::Status
    })
}

struct TerminalGuard {
    terminal: Terminal<CrosstermBackend<Stdout>>,
}

impl TerminalGuard {
    fn new() -> Result<Self> {
        enable_raw_mode()?;
        let mut stdout = io::stdout();
        if let Err(error) = execute!(
            stdout,
            EnterAlternateScreen,
            EnableMouseCapture,
            EnableFocusChange,
            SetTitle("RepoRadar"),
            SetCursorStyle::SteadyBlock
        ) {
            let _ = disable_raw_mode();
            let mut cleanup = io::stdout();
            let _ = execute!(
                cleanup,
                DisableFocusChange,
                DisableMouseCapture,
                LeaveAlternateScreen,
                SetCursorStyle::DefaultUserShape,
                SetTitle("")
            );
            return Err(error.into());
        }
        let terminal = match Terminal::new(CrosstermBackend::new(stdout)) {
            Ok(terminal) => terminal,
            Err(error) => {
                let _ = disable_raw_mode();
                let mut cleanup = io::stdout();
                let _ = execute!(
                    cleanup,
                    DisableFocusChange,
                    DisableMouseCapture,
                    LeaveAlternateScreen,
                    SetCursorStyle::DefaultUserShape,
                    SetTitle("")
                );
                return Err(error.into());
            }
        };
        Ok(Self { terminal })
    }
}

impl Drop for TerminalGuard {
    fn drop(&mut self) {
        let _ = disable_raw_mode();
        let _ = execute!(
            self.terminal.backend_mut(),
            DisableFocusChange,
            DisableMouseCapture,
            LeaveAlternateScreen,
            SetCursorStyle::DefaultUserShape,
            SetTitle("")
        );
        let _ = self.terminal.show_cursor();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn repository(root: &str, name: &str) -> Repository {
        Repository {
            root: PathBuf::from(root),
            name: name.to_owned(),
            branch: "main".to_owned(),
            files: Vec::new(),
            error: None,
        }
    }

    #[test]
    fn refresh_interval_scales_with_workspace_size() {
        assert_eq!(refresh_interval(25), Duration::from_secs(2));
        assert_eq!(refresh_interval(26), Duration::from_secs(5));
        assert_eq!(refresh_interval(100), Duration::from_secs(5));
        assert_eq!(refresh_interval(101), Duration::from_secs(10));
    }

    #[test]
    fn scheduled_scans_do_not_queue_behind_an_active_scan() {
        let now = Instant::now();
        assert_eq!(scheduled_scan_mode(now, now, now, true), None,);
    }

    #[test]
    fn scheduled_scans_rediscover_only_after_the_discovery_deadline() {
        let now = Instant::now();
        assert_eq!(
            scheduled_scan_mode(
                now,
                now - Duration::from_secs(1),
                now + Duration::from_secs(1),
                false,
            ),
            Some(ScanMode::Status),
        );
        assert_eq!(
            scheduled_scan_mode(now, now, now, false),
            Some(ScanMode::Discover),
        );
    }

    #[test]
    fn scan_errors_are_bounded_and_terminal_safe() {
        let error = anyhow::anyhow!("{}\u{1b}[2J", "x".repeat(200));
        let rendered = bound_scan_error(error);

        assert!(!rendered.contains('\u{1b}'));
        assert_eq!(rendered.chars().count(), 163);
        assert!(rendered.ends_with("..."));
    }

    #[test]
    fn duplicate_leaf_names_use_workspace_relative_paths() {
        let mut repositories = vec![
            repository("/workspace/services/api", "api"),
            repository("/workspace/legacy/api", "api"),
            repository("/workspace/web", "web"),
        ];

        disambiguate_names(Path::new("/workspace"), &mut repositories);

        assert_eq!(repositories[0].name, "services/api");
        assert_eq!(repositories[1].name, "legacy/api");
        assert_eq!(repositories[2].name, "web");
    }
}
