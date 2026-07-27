use std::ffi::OsStr;
use std::io;
use std::path::{Path, PathBuf};

use ignore::WalkBuilder;

#[derive(Debug, Eq, PartialEq)]
pub struct Discovery {
    pub repositories: Vec<PathBuf>,
    pub skipped_paths: usize,
}

pub fn discover_repositories(root: &Path) -> io::Result<Discovery> {
    if !root.is_dir() {
        return Err(io::Error::new(
            io::ErrorKind::NotFound,
            format!("workspace is not a directory: {}", root.display()),
        ));
    }

    let workspace_root = root.to_path_buf();
    let mut builder = WalkBuilder::new(root);
    builder
        .follow_links(false)
        .hidden(false)
        .standard_filters(false)
        .filter_entry(move |entry| {
            entry.path() == workspace_root || should_descend(entry.path(), entry.file_name())
        });

    let mut repositories = Vec::new();
    let mut skipped_paths = 0;
    for entry in builder.build() {
        match entry {
            Ok(entry) => {
                let path = entry.path();
                if entry.file_type().is_some_and(|kind| kind.is_dir()) {
                    match path.join(".git").try_exists() {
                        Ok(true) => repositories.push(path.to_path_buf()),
                        Ok(false) => {}
                        Err(_) => skipped_paths += 1,
                    }
                }
            }
            Err(_) => skipped_paths += 1,
        }
    }

    repositories.sort_by(|left, right| {
        left.to_string_lossy()
            .to_lowercase()
            .cmp(&right.to_string_lossy().to_lowercase())
            .then_with(|| left.cmp(right))
    });
    repositories.dedup();
    Ok(Discovery {
        repositories,
        skipped_paths,
    })
}

fn should_descend(path: &Path, name: &OsStr) -> bool {
    if name == OsStr::new(".git") {
        return false;
    }
    const GENERATED: &[&str] = &[
        ".cache",
        ".next",
        ".turbo",
        ".venv",
        "build",
        "coverage",
        "dist",
        "node_modules",
        "target",
        "vendor",
        "venv",
    ];
    let generated = GENERATED
        .iter()
        .any(|candidate| name == OsStr::new(candidate));
    !generated || path.join(".git").try_exists().unwrap_or(true)
}

#[cfg(test)]
mod tests {
    use std::fs;

    use tempfile::tempdir;

    use super::*;

    #[test]
    fn discovers_root_and_nested_repositories_without_git_internals() {
        let temp = tempdir().unwrap();
        fs::create_dir(temp.path().join(".git")).unwrap();
        fs::create_dir_all(temp.path().join("group/repo/.git")).unwrap();
        fs::create_dir_all(temp.path().join("group/repo/.git/fake/.git")).unwrap();

        let discovery = discover_repositories(temp.path()).unwrap();

        assert_eq!(discovery.repositories.len(), 2);
        assert!(discovery.repositories.contains(&temp.path().to_path_buf()));
        assert!(
            discovery
                .repositories
                .contains(&temp.path().join("group/repo"))
        );
        assert_eq!(discovery.skipped_paths, 0);
    }

    #[test]
    fn recognizes_worktree_git_files() {
        let temp = tempdir().unwrap();
        fs::create_dir_all(temp.path().join("worktree")).unwrap();
        fs::write(temp.path().join("worktree/.git"), "gitdir: elsewhere").unwrap();

        assert_eq!(
            discover_repositories(temp.path()).unwrap().repositories,
            vec![temp.path().join("worktree")]
        );
    }

    #[test]
    fn parent_ignore_rules_do_not_hide_independent_repositories() {
        let temp = tempdir().unwrap();
        fs::create_dir(temp.path().join(".git")).unwrap();
        fs::write(temp.path().join(".gitignore"), "repos/\n").unwrap();
        fs::create_dir_all(temp.path().join("repos/child/.git")).unwrap();

        let discovery = discover_repositories(temp.path()).unwrap();

        assert!(
            discovery
                .repositories
                .contains(&temp.path().join("repos/child"))
        );
    }

    #[test]
    fn prunes_generated_trees_but_keeps_a_repository_with_a_generated_name() {
        let temp = tempdir().unwrap();
        fs::create_dir_all(temp.path().join("node_modules/dependency/.git")).unwrap();
        fs::create_dir_all(temp.path().join("target/.git")).unwrap();

        assert_eq!(
            discover_repositories(temp.path()).unwrap().repositories,
            vec![temp.path().join("target")]
        );
    }

    #[test]
    fn rejects_a_missing_workspace_root() {
        let temp = tempdir().unwrap();
        let missing = temp.path().join("missing");
        assert_eq!(
            discover_repositories(&missing).unwrap_err().kind(),
            io::ErrorKind::NotFound
        );
    }
}
