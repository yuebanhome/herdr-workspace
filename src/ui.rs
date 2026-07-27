use ratatui::Frame;
use ratatui::layout::{Constraint, Layout};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{List, ListItem, Paragraph};

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

    if let Some(error) = &state.error {
        frame.render_widget(
            Paragraph::new(Line::from(vec![
                Span::styled("! ", Style::default().fg(Color::Red)),
                Span::raw(error),
            ])),
            body,
        );
    } else if state.repositories.is_empty() {
        let message = if state.scanning {
            "Scanning workspace..."
        } else {
            "No Git repositories"
        };
        frame.render_widget(
            Paragraph::new(message).style(Style::default().fg(Color::DarkGray)),
            body,
        );
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
            .map(|row| render_row(state, *row))
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

    let summary = format!(
        "{} changed / {} repos",
        state.dirty_count(),
        state.repositories.len()
    );
    frame.render_widget(
        Paragraph::new(summary).style(Style::default().fg(Color::DarkGray)),
        footer,
    );
}

fn render_row(state: &AppState, row: RowKind) -> ListItem<'static> {
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
            let line = Line::from(vec![
                Span::styled(format!(" {indicator} "), indicator_style),
                Span::styled(repository.name.clone(), name_style),
                Span::styled(count, Style::default().fg(Color::Yellow)),
            ]);
            ListItem::new(line).style(selected_style(selected))
        }
        RowKind::Branch(index) => {
            let repository = &state.repositories[index];
            let branch = repository.error.as_deref().unwrap_or(&repository.branch);
            ListItem::new(Line::from(Span::styled(
                format!("    {branch}"),
                Style::default().fg(Color::DarkGray),
            )))
        }
        RowKind::File(repo_index, file_index) => {
            let change = &state.repositories[repo_index].files[file_index];
            let marker = change.marker();
            let color = match marker {
                '?' => Color::Cyan,
                'D' | 'U' => Color::Red,
                'A' => Color::Green,
                _ => Color::Yellow,
            };
            ListItem::new(Line::from(vec![
                Span::styled(format!("    {marker} "), Style::default().fg(color)),
                Span::raw(change.path.clone()),
            ]))
        }
    }
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
        let backend = TestBackend::new(30, 8);
        let mut terminal = Terminal::new(backend).unwrap();

        terminal.draw(|frame| draw(frame, &mut state)).unwrap();

        let rendered = terminal.backend().to_string();
        assert!(rendered.contains("clean"));
        assert!(rendered.contains("dirty 1"));
        assert!(rendered.contains("M src/main.rs"));
        assert!(rendered.contains("1 changed / 2 repos"));
    }
}
