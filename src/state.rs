use std::collections::HashSet;
use std::path::{Path, PathBuf};

use crate::model::Repository;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RowKind {
    Repository(usize),
    Branch(usize),
    File(usize, usize),
}

#[derive(Debug)]
pub struct AppState {
    pub root: PathBuf,
    pub repositories: Vec<Repository>,
    pub selected: usize,
    pub expanded: HashSet<PathBuf>,
    pub scroll: usize,
    pub scanning: bool,
    pub scan_queued: bool,
    pub error: Option<String>,
    pub hit_rows: Vec<(u16, usize, bool)>,
    pub body_height: usize,
}

impl AppState {
    pub fn new(root: PathBuf) -> Self {
        Self {
            root,
            repositories: Vec::new(),
            selected: 0,
            expanded: HashSet::new(),
            scroll: 0,
            scanning: false,
            scan_queued: false,
            error: None,
            hit_rows: Vec::new(),
            body_height: 0,
        }
    }

    pub fn replace_repositories(&mut self, repositories: Vec<Repository>) {
        let selected_path = self
            .repositories
            .get(self.selected)
            .map(|repo| repo.root.clone());
        self.repositories = repositories;
        self.selected = selected_path
            .and_then(|path| self.repositories.iter().position(|repo| repo.root == path))
            .unwrap_or(0)
            .min(self.repositories.len().saturating_sub(1));
        let current_paths: HashSet<&Path> = self
            .repositories
            .iter()
            .map(|repo| repo.root.as_path())
            .collect();
        self.expanded
            .retain(|path| current_paths.contains(path.as_path()));
        self.clamp_scroll();
    }

    pub fn dirty_count(&self) -> usize {
        self.repositories
            .iter()
            .filter(|repo| repo.is_dirty())
            .count()
    }

    pub fn rows(&self) -> Vec<RowKind> {
        let mut rows = Vec::new();
        for (repo_index, repository) in self.repositories.iter().enumerate() {
            rows.push(RowKind::Repository(repo_index));
            rows.push(RowKind::Branch(repo_index));
            if self.expanded.contains(&repository.root) {
                rows.extend(
                    repository
                        .files
                        .iter()
                        .enumerate()
                        .map(|(file_index, _)| RowKind::File(repo_index, file_index)),
                );
            }
        }
        rows
    }

    pub fn select(&mut self, index: usize) {
        if !self.repositories.is_empty() {
            self.selected = index.min(self.repositories.len() - 1);
            self.ensure_selected_visible();
        }
    }

    pub fn move_selection(&mut self, delta: isize) {
        if self.repositories.is_empty() {
            return;
        }
        self.selected = self
            .selected
            .saturating_add_signed(delta)
            .min(self.repositories.len() - 1);
        self.ensure_selected_visible();
    }

    pub fn select_first(&mut self) {
        self.select(0);
    }

    pub fn select_last(&mut self) {
        self.select(self.repositories.len().saturating_sub(1));
    }

    pub fn toggle_selected(&mut self) {
        let Some(repository) = self.repositories.get(self.selected) else {
            return;
        };
        if repository.files.is_empty() {
            return;
        }
        if !self.expanded.remove(&repository.root) {
            self.expanded.insert(repository.root.clone());
        }
        self.ensure_selected_visible();
    }

    pub fn click(&mut self, terminal_row: u16) {
        if let Some((_, repository, toggles)) = self
            .hit_rows
            .iter()
            .find(|(row, _, _)| *row == terminal_row)
            .copied()
        {
            self.select(repository);
            if toggles {
                self.toggle_selected();
            }
        }
    }

    pub fn ensure_selected_visible(&mut self) {
        if self.body_height == 0 || self.repositories.is_empty() {
            return;
        }
        let rows = self.rows();
        let selected_row = rows
            .iter()
            .position(|row| *row == RowKind::Repository(self.selected))
            .unwrap_or(0);
        if selected_row < self.scroll {
            self.scroll = selected_row;
        } else if selected_row >= self.scroll + self.body_height {
            self.scroll = selected_row + 1 - self.body_height;
        }
        self.clamp_scroll();
    }

    pub fn clamp_scroll(&mut self) {
        let max_scroll = self.rows().len().saturating_sub(self.body_height.max(1));
        self.scroll = self.scroll.min(max_scroll);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::FileChange;

    fn repository(name: &str, dirty: bool) -> Repository {
        Repository {
            root: PathBuf::from(name),
            name: name.to_owned(),
            branch: "main".to_owned(),
            files: dirty
                .then(|| FileChange {
                    path: "file.rs".to_owned(),
                    original_path: None,
                    index_status: ' ',
                    worktree_status: 'M',
                })
                .into_iter()
                .collect(),
            error: None,
        }
    }

    #[test]
    fn preserves_selection_and_expansion_by_path() {
        let mut state = AppState::new(PathBuf::from("root"));
        state.replace_repositories(vec![repository("a", false), repository("b", true)]);
        state.select(1);
        state.toggle_selected();

        state.replace_repositories(vec![repository("a", true), repository("b", true)]);

        assert_eq!(state.selected, 1);
        assert!(state.expanded.contains(Path::new("b")));
    }

    #[test]
    fn expanded_files_are_part_of_visible_rows() {
        let mut state = AppState::new(PathBuf::from("root"));
        state.replace_repositories(vec![repository("a", true)]);
        state.toggle_selected();

        assert_eq!(
            state.rows(),
            vec![
                RowKind::Repository(0),
                RowKind::Branch(0),
                RowKind::File(0, 0)
            ]
        );
    }

    #[test]
    fn clicking_a_repository_row_selects_and_toggles_it() {
        let mut state = AppState::new(PathBuf::from("root"));
        state.replace_repositories(vec![repository("a", false), repository("b", true)]);
        state.hit_rows = vec![(5, 1, true)];

        state.click(5);

        assert_eq!(state.selected, 1);
        assert!(state.expanded.contains(Path::new("b")));
    }
}
