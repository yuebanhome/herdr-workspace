use std::fs;
use std::path::Path;
use std::process::Command;

use herdr_reporadar::app::scan_workspace;
use tempfile::tempdir;

fn git(root: &Path, args: &[&str]) {
    let output = Command::new("git")
        .arg("-C")
        .arg(root)
        .args(args)
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "git {args:?} failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
}

fn init_repository(root: &Path) {
    fs::create_dir_all(root).unwrap();
    git(root, &["init", "-q"]);
    git(root, &["config", "user.name", "RepoRadar Test"]);
    git(root, &["config", "user.email", "reporadar@example.invalid"]);
    fs::write(root.join("modified.txt"), "before\n").unwrap();
    fs::write(root.join("deleted.txt"), "delete me\n").unwrap();
    fs::write(root.join("renamed.txt"), "rename me\n").unwrap();
    git(root, &["add", "."]);
    git(root, &["commit", "-qm", "initial"]);
}

#[test]
fn scans_real_repositories_and_reports_file_level_changes() {
    let temp = tempdir().unwrap();
    let clean = temp.path().join("clean");
    let dirty = temp.path().join("group/dirty");
    init_repository(&clean);
    init_repository(&dirty);

    fs::write(dirty.join("modified.txt"), "after\n").unwrap();
    fs::remove_file(dirty.join("deleted.txt")).unwrap();
    fs::write(dirty.join("untracked.txt"), "new\n").unwrap();
    git(&dirty, &["mv", "renamed.txt", "moved.txt"]);

    let repositories = scan_workspace(temp.path()).unwrap();

    assert_eq!(repositories.len(), 2);
    let clean = repositories
        .iter()
        .find(|repo| repo.name == "clean")
        .unwrap();
    assert!(!clean.is_dirty());
    let dirty = repositories
        .iter()
        .find(|repo| repo.name == "dirty")
        .unwrap();
    assert_eq!(dirty.files.len(), 4);
    assert_eq!(
        dirty
            .files
            .iter()
            .map(|file| (file.path.as_str(), file.marker()))
            .collect::<Vec<_>>(),
        vec![
            ("deleted.txt", 'D'),
            ("modified.txt", 'M'),
            ("moved.txt", 'R'),
            ("untracked.txt", '?'),
        ]
    );
}

#[test]
fn an_unpushed_commit_does_not_make_a_repository_dirty() {
    let temp = tempdir().unwrap();
    let repository = temp.path().join("repo");
    init_repository(&repository);
    fs::write(repository.join("committed.txt"), "committed\n").unwrap();
    git(&repository, &["add", "committed.txt"]);
    git(&repository, &["commit", "-qm", "local commit"]);

    let repositories = scan_workspace(temp.path()).unwrap();

    assert_eq!(repositories.len(), 1);
    assert!(!repositories[0].is_dirty());
}
