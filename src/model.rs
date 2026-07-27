use std::path::PathBuf;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct FileChange {
    pub path: String,
    pub original_path: Option<String>,
    pub index_status: char,
    pub worktree_status: char,
}

impl FileChange {
    pub fn marker(&self) -> char {
        let pair = [self.index_status, self.worktree_status];
        if matches!(
            pair,
            ['D', 'D']
                | ['A', 'U']
                | ['U', 'D']
                | ['U', 'A']
                | ['D', 'U']
                | ['A', 'A']
                | ['U', 'U']
        ) {
            return 'U';
        }
        if pair == ['?', '?'] {
            return '?';
        }
        for marker in ['R', 'D', 'A', 'C', 'T', 'M'] {
            if pair.contains(&marker) {
                return marker;
            }
        }
        pair.into_iter()
            .find(|status| !matches!(status, ' ' | '!'))
            .unwrap_or('?')
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Repository {
    pub root: PathBuf,
    pub name: String,
    pub branch: String,
    pub files: Vec<FileChange>,
    pub error: Option<String>,
}

impl Repository {
    pub fn is_dirty(&self) -> bool {
        !self.files.is_empty()
    }
}
