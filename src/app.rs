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
use crate::git::read_repository;
use crate::model::Repository;
use crate::state::AppState;
use crate::ui;

const EVENT_TICK: Duration = Duration::from_millis(100);
const REFRESH_INTERVAL: Duration = Duration::from_secs(2);

type ScanResult = std::result::Result<Vec<Repository>, String>;

pub fn run(root: PathBuf) -> Result<()> {
    let mut terminal = TerminalGuard::new()?;
    let (sender, receiver) = mpsc::channel();
    let mut state = AppState::new(root);
    start_scan(&mut state, sender.clone());
    let mut next_refresh = Instant::now() + REFRESH_INTERVAL;

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
                    KeyCode::Char('r') => request_scan(&mut state, sender.clone()),
                    _ => {}
                },
                Event::Mouse(mouse) => match mouse.kind {
                    MouseEventKind::Down(MouseButton::Left) => state.click(mouse.row),
                    MouseEventKind::ScrollDown => state.move_selection(3),
                    MouseEventKind::ScrollUp => state.move_selection(-3),
                    _ => {}
                },
                Event::FocusGained => request_scan(&mut state, sender.clone()),
                Event::Resize(_, _) => state.ensure_selected_visible(),
                _ => {}
            }
        }

        while let Ok(result) = receiver.try_recv() {
            state.scanning = false;
            match result {
                Ok(repositories) => {
                    state.error = None;
                    state.replace_repositories(repositories);
                }
                Err(error) => state.error = Some(error),
            }
            if state.scan_queued {
                state.scan_queued = false;
                start_scan(&mut state, sender.clone());
            }
            next_refresh = Instant::now() + REFRESH_INTERVAL;
        }

        if Instant::now() >= next_refresh {
            request_scan(&mut state, sender.clone());
            next_refresh = Instant::now() + REFRESH_INTERVAL;
        }
    }
    Ok(())
}

fn request_scan(state: &mut AppState, sender: mpsc::Sender<ScanResult>) {
    if state.scanning {
        state.scan_queued = true;
    } else {
        start_scan(state, sender);
    }
}

fn start_scan(state: &mut AppState, sender: mpsc::Sender<ScanResult>) {
    state.scanning = true;
    let root = state.root.clone();
    thread::spawn(move || {
        let result = scan_workspace(&root).map_err(|error| error.to_string());
        let _ = sender.send(result);
    });
}

pub fn scan_workspace(root: &Path) -> Result<Vec<Repository>> {
    let paths = discover_repositories(root)
        .with_context(|| format!("could not scan workspace {}", root.display()))?;
    if paths.is_empty() {
        return Ok(Vec::new());
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
    repositories.sort_by(|left, right| {
        left.name
            .to_lowercase()
            .cmp(&right.name.to_lowercase())
            .then_with(|| left.root.cmp(&right.root))
    });
    Ok(repositories)
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
            return Err(error.into());
        }
        let terminal = Terminal::new(CrosstermBackend::new(stdout))?;
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
            SetTitle("")
        );
        let _ = self.terminal.show_cursor();
    }
}
