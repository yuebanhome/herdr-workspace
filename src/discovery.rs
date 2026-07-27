use std::ffi::OsStr;
use std::io;
use std::path::{Path, PathBuf};

use ignore::WalkBuilder;

pub fn discover_repositories(root: &Path) -> io::Result<Vec<PathBuf>> {
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
    for entry in builder.build().filter_map(Result::ok) {
        let path = entry.path();
        if entry.file_type().is_some_and(|kind| kind.is_dir()) && path.join(".git").exists() {
            repositories.push(path.to_path_buf());
        }
    }

    repositories.sort_by(|left, right| {
        left.to_string_lossy()
            .to_lowercase()
            .cmp(&right.to_string_lossy().to_lowercase())
            .then_with(|| left.cmp(right))
    });
    repositories.dedup();
    Ok(repositories)
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
    !generated || path.join(".git").exists()
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

        let repos = discover_repositories(temp.path()).unwrap();

        assert_eq!(repos.len(), 2);
        assert!(repos.contains(&temp.path().to_path_buf()));
        assert!(repos.contains(&temp.path().join("group/repo")));
    }

    #[test]
    fn recognizes_worktree_git_files() {
        let temp = tempdir().unwrap();
        fs::create_dir_all(temp.path().join("worktree")).unwrap();
        fs::write(temp.path().join("worktree/.git"), "gitdir: elsewhere").unwrap();

        assert_eq!(
            discover_repositories(temp.path()).unwrap(),
            vec![temp.path().join("worktree")]
        );
    }

    #[test]
    fn parent_ignore_rules_do_not_hide_independent_repositories() {
        let temp = tempdir().unwrap();
        fs::create_dir(temp.path().join(".git")).unwrap();
        fs::write(temp.path().join(".gitignore"), "repos/\n").unwrap();
        fs::create_dir_all(temp.path().join("repos/child/.git")).unwrap();

        let repos = discover_repositories(temp.path()).unwrap();

        assert!(repos.contains(&temp.path().join("repos/child")));
    }

    #[test]
    fn prunes_generated_trees_but_keeps_a_repository_with_a_generated_name() {
        let temp = tempdir().unwrap();
        fs::create_dir_all(temp.path().join("node_modules/dependency/.git")).unwrap();
        fs::create_dir_all(temp.path().join("target/.git")).unwrap();

        assert_eq!(
            discover_repositories(temp.path()).unwrap(),
            vec![temp.path().join("target")]
        );
    }
}
