use ratatui::Frame;
use ratatui::layout::{Constraint, Layout};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{List, ListItem, Paragraph};
use unicode_width::{UnicodeWidthChar, UnicodeWidthStr};

use crate::host;
use crate::state::{AppState, RowKind};

pub fn draw(frame: &mut Frame<'_>, state: &mut AppState) {
    let area = frame.area();
    let [header, body, footer] = Layout::vertical([
        Constraint::Length(1),
        Constraint::Min(1),
        Constraint::Length(1),
    ])
    .areas(area);

    let root = host::display_root(&state.root);
    let scan = if state.scanning { "  scanning" } else { "" };
    frame.render_widget(
        Paragraph::new(Line::from(vec![
            Span::styled(root, Style::default().add_modifier(Modifier::BOLD)),
            Span::styled(scan, Style::default().fg(Color::DarkGray)),
        ])),
        header,
    );

    state.body_height = body.height as usize;
    state.ensure_selected_visible();
    state.hit_rows.clear();

    if state.repositories.is_empty() {
        if let Some(error) = &state.error {
            frame.render_widget(
                Paragraph::new(Line::from(vec![
                    Span::styled("! ", Style::default().fg(Color::Red)),
                    Span::raw(error),
                ])),
                body,
            );
        } else {
            let message = if state.scanning {
                "Scanning workspace..."
            } else {
                "No Git repositories"
            };
            frame.render_widget(
                Paragraph::new(message).style(Style::default().fg(Color::DarkGray)),
                body,
            );
        }
    } else {
        let rows = state.rows();
        state.clamp_scroll();
        let visible = rows
            .iter()
            .copied()
            .skip(state.scroll)
            .take(body.height as usize)
            .collect::<Vec<_>>();
        let items = visible
            .iter()
            .map(|row| render_row(state, *row, body.width as usize))
            .collect::<Vec<_>>();
        for (offset, row) in visible.iter().enumerate() {
            let (repo, toggles) = match row {
                RowKind::Repository(repo) => (*repo, true),
                RowKind::Branch(repo) => (*repo, true),
                RowKind::File(repo, _) => (*repo, false),
            };
            state.hit_rows.push((body.y + offset as u16, repo, toggles));
        }
        frame.render_widget(List::new(items), body);
    }

    frame.render_widget(
        Paragraph::new(footer_line(state, footer.width as usize)),
        footer,
    );
}

fn footer_line(state: &AppState, width: usize) -> Line<'static> {
    let normal = format!(
        "{} changed / {} repos",
        state.dirty_count(),
        state.repositories.len()
    );
    let warning = state
        .error
        .as_ref()
        .map(|error| format!("! {error}"))
        .or_else(|| {
            (state.skipped_paths > 0).then(|| format!("! {} paths skipped", state.skipped_paths))
        });
    let Some(warning) = warning else {
        return Line::from(Span::styled(normal, Style::default().fg(Color::DarkGray)));
    };

    let compact = format!(
        "{}/{} changed",
        state.dirty_count(),
        state.repositories.len()
    );
    let summary = if normal.chars().count() + 2 + warning.chars().count() <= width {
        Some(normal)
    } else if compact.chars().count() + 2 + warning.chars().count() <= width {
        Some(compact)
    } else {
        None
    };

    let mut spans = Vec::with_capacity(2);
    if let Some(summary) = summary {
        spans.push(Span::styled(
            format!("{summary}  "),
            Style::default().fg(Color::DarkGray),
        ));
    }
    spans.push(Span::styled(warning, Style::default().fg(Color::Yellow)));
    Line::from(spans)
}

fn render_row(state: &AppState, row: RowKind, width: usize) -> ListItem<'static> {
    match row {
        RowKind::Repository(index) => {
            let repository = &state.repositories[index];
            let selected = state.selected == index;
            let (indicator, indicator_style, name_style) = if repository.error.is_some() {
                (
                    "!",
                    Style::default().fg(Color::Red).add_modifier(Modifier::BOLD),
                    Style::default().fg(Color::Red),
                )
            } else if repository.is_dirty() {
                (
                    "●",
                    Style::default().fg(Color::Yellow),
                    Style::default().add_modifier(Modifier::BOLD),
                )
            } else {
                (
                    "○",
                    Style::default().fg(Color::Green),
                    Style::default().fg(Color::DarkGray),
                )
            };
            let name_style = if selected {
                name_style.fg(Color::White).add_modifier(Modifier::BOLD)
            } else {
                name_style
            };
            let count = if repository.is_dirty() {
                format!(" {}", repository.files.len())
            } else {
                String::new()
            };
            let fixed_width = 3 + UnicodeWidthStr::width(count.as_str());
            let name = fit_middle(&repository.name, width.saturating_sub(fixed_width));
            let line = Line::from(vec![
                Span::styled(format!(" {indicator} "), indicator_style),
                Span::styled(name, name_style),
                Span::styled(count, Style::default().fg(Color::Yellow)),
            ]);
            ListItem::new(line).style(selected_style(selected))
        }
        RowKind::Branch(index) => {
            let repository = &state.repositories[index];
            let branch = repository.error.as_deref().unwrap_or(&repository.branch);
            ListItem::new(Line::from(Span::styled(
                format!("    {}", fit_middle(branch, width.saturating_sub(4))),
                Style::default().fg(Color::DarkGray),
            )))
        }
        RowKind::File(repo_index, file_index) => {
            let repository = &state.repositories[repo_index];
            let change = &repository.files[file_index];
            let marker = change.marker();
            let color = match marker {
                '?' => Color::Cyan,
                'D' | 'U' => Color::Red,
                'A' => Color::Green,
                _ => Color::Yellow,
            };
            ListItem::new(Line::from(vec![
                Span::styled(format!("    {marker} "), Style::default().fg(color)),
                Span::raw(fit_middle(
                    &concise_file_label(repository, file_index),
                    width.saturating_sub(6),
                )),
            ]))
        }
    }
}

fn concise_file_label(repository: &crate::model::Repository, file_index: usize) -> String {
    let path = &repository.files[file_index].path;
    let components = path.split('/').collect::<Vec<_>>();
    for depth in 1..=components.len() {
        let suffix = components[components.len() - depth..].join("/");
        let unique = repository
            .files
            .iter()
            .enumerate()
            .all(|(other_index, other)| {
                other_index == file_index || !has_path_suffix(&other.path, &suffix)
            });
        if unique {
            return if depth == 1 {
                strip_directory_common_prefix(repository, file_index, &suffix)
            } else {
                suffix
            };
        }
    }
    path.clone()
}

fn strip_directory_common_prefix(
    repository: &crate::model::Repository,
    file_index: usize,
    basename: &str,
) -> String {
    let path = &repository.files[file_index].path;
    let parent = path.rsplit_once('/').map_or("", |(parent, _)| parent);
    let peers = repository.files.iter().filter_map(|file| {
        let (other_parent, other_basename) = file.path.rsplit_once('/').unwrap_or(("", &file.path));
        (other_parent == parent).then_some(other_basename)
    });
    let mut peer_count = 0;
    let mut prefix_bytes = basename.len();
    for peer in peers {
        peer_count += 1;
        prefix_bytes = prefix_bytes.min(common_prefix_bytes(basename, peer));
    }
    let prefix_ends_at_separator = basename[..prefix_bytes]
        .chars()
        .last()
        .is_some_and(|character| matches!(character, '.' | '-' | '_'));
    if peer_count < 2 || prefix_bytes < 3 || !prefix_ends_at_separator {
        return basename.to_owned();
    }

    let concise = basename[prefix_bytes..].trim_start_matches(['.', '-', '_']);
    if concise.is_empty() {
        basename.to_owned()
    } else {
        concise.to_owned()
    }
}

fn common_prefix_bytes(left: &str, right: &str) -> usize {
    left.chars()
        .zip(right.chars())
        .take_while(|(left, right)| left == right)
        .map(|(character, _)| character.len_utf8())
        .sum()
}

fn has_path_suffix(path: &str, suffix: &str) -> bool {
    path == suffix
        || path
            .strip_suffix(suffix)
            .is_some_and(|prefix| prefix.ends_with('/'))
}

fn fit_middle(value: &str, max_width: usize) -> String {
    if UnicodeWidthStr::width(value) <= max_width {
        return value.to_owned();
    }
    if max_width <= 3 {
        return ".".repeat(max_width);
    }

    let content_width = max_width - 3;
    let prefix_width = content_width * 3 / 4;
    let suffix_width = content_width - prefix_width;
    format!(
        "{}...{}",
        take_prefix(value, prefix_width),
        take_suffix(value, suffix_width)
    )
}

fn take_prefix(value: &str, max_width: usize) -> String {
    let mut width = 0;
    value
        .chars()
        .take_while(|character| {
            let character_width = character.width().unwrap_or(0);
            if width + character_width > max_width {
                false
            } else {
                width += character_width;
                true
            }
        })
        .collect()
}

fn take_suffix(value: &str, max_width: usize) -> String {
    let mut width = 0;
    let mut characters = value
        .chars()
        .rev()
        .take_while(|character| {
            let character_width = character.width().unwrap_or(0);
            if width + character_width > max_width {
                false
            } else {
                width += character_width;
                true
            }
        })
        .collect::<Vec<_>>();
    characters.reverse();
    characters.into_iter().collect()
}

fn selected_style(selected: bool) -> Style {
    if selected {
        Style::default().bg(Color::DarkGray)
    } else {
        Style::default()
    }
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use ratatui::Terminal;
    use ratatui::backend::TestBackend;

    use crate::model::{FileChange, Repository};

    use super::*;

    #[test]
    fn renders_clean_dirty_and_expanded_repositories_in_a_narrow_pane() {
        let mut state = AppState::new(PathBuf::from("/workspace"));
        state.replace_repositories(vec![
            Repository {
                root: PathBuf::from("/workspace/clean"),
                name: "clean".to_owned(),
                branch: "main".to_owned(),
                files: Vec::new(),
                error: None,
            },
            Repository {
                root: PathBuf::from("/workspace/dirty"),
                name: "dirty".to_owned(),
                branch: "feature".to_owned(),
                files: vec![FileChange {
                    path: "src/main.rs".to_owned(),
                    original_path: None,
                    index_status: ' ',
                    worktree_status: 'M',
                }],
                error: None,
            },
        ]);
        state.select(1);
        state.toggle_selected();
        state.skipped_paths = 2;
        let backend = TestBackend::new(30, 8);
        let mut terminal = Terminal::new(backend).unwrap();

        terminal.draw(|frame| draw(frame, &mut state)).unwrap();

        let rendered = terminal.backend().to_string();
        assert!(rendered.contains("clean"));
        assert!(rendered.contains("dirty 1"));
        assert!(rendered.contains("M main.rs"));
        assert!(rendered.contains("1/2 changed"));
        assert!(rendered.contains("2 paths skipped"));

        state.error = Some("workspace scan failed".to_owned());
        terminal.draw(|frame| draw(frame, &mut state)).unwrap();
        let rendered = terminal.backend().to_string();
        assert!(rendered.contains("dirty"));
        assert!(rendered.contains("workspace scan failed"));
    }

    #[test]
    fn preserves_path_context_and_filename_when_space_is_tight() {
        let rendered = fit_middle("codex-rs/tui/src/status/long_filename.rs", 24);

        assert!(UnicodeWidthStr::width(rendered.as_str()) <= 24);
        assert!(rendered.starts_with("codex"));
        assert!(rendered.ends_with(".rs"));
        assert!(rendered.contains("..."));
    }

    #[test]
    fn truncated_labels_keep_similar_names_distinguishable() {
        let unix = fit_middle("standalone_unix_update_available_snapshot.snap", 25);
        let windows = fit_middle("standalone_windows_update_available_snapshot.snap", 25);

        assert_ne!(unix, windows);
        assert!(unix.contains("unix"));
        assert!(windows.contains("windo"));
    }

    #[test]
    fn file_labels_use_the_shortest_unique_path_suffix() {
        let mut repository = Repository {
            root: PathBuf::from("/workspace/repo"),
            name: "repo".to_owned(),
            branch: "main".to_owned(),
            files: vec![
                FileChange {
                    path: "src/api/common.rs".to_owned(),
                    original_path: None,
                    index_status: ' ',
                    worktree_status: 'M',
                },
                FileChange {
                    path: "src/ui/common.rs".to_owned(),
                    original_path: None,
                    index_status: ' ',
                    worktree_status: 'M',
                },
                FileChange {
                    path: "src/unique.rs".to_owned(),
                    original_path: None,
                    index_status: ' ',
                    worktree_status: 'M',
                },
            ],
            error: None,
        };

        assert_eq!(concise_file_label(&repository, 0), "api/common.rs");
        assert_eq!(concise_file_label(&repository, 1), "ui/common.rs");
        assert_eq!(concise_file_label(&repository, 2), "unique.rs");

        repository.files[1].path = "src/api/common.rs".to_owned();
        assert_eq!(concise_file_label(&repository, 0), "src/api/common.rs");
    }

    #[test]
    fn file_labels_remove_generated_prefixes_shared_within_a_directory() {
        let repository = Repository {
            root: PathBuf::from("/workspace/repo"),
            name: "repo".to_owned(),
            branch: "main".to_owned(),
            files: vec![
                FileChange {
                    path: "snapshots/codex_tui__status__tests__cached_limits.snap".to_owned(),
                    original_path: None,
                    index_status: ' ',
                    worktree_status: 'M',
                },
                FileChange {
                    path: "snapshots/codex_tui__status__tests__monthly_limit.snap".to_owned(),
                    original_path: None,
                    index_status: ' ',
                    worktree_status: 'M',
                },
            ],
            error: None,
        };

        assert_eq!(concise_file_label(&repository, 0), "cached_limits.snap");
        assert_eq!(concise_file_label(&repository, 1), "monthly_limit.snap");
    }
}
