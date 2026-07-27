use std::path::Path;
use std::process::Command;

use anyhow::{Context, Result, bail};

use crate::model::{FileChange, Repository};

pub fn read_repository(root: &Path) -> Repository {
    match run_status(root) {
        Ok((branch, files)) => Repository {
            root: root.to_path_buf(),
            name: repository_name(root),
            branch,
            files,
            error: None,
        },
        Err(error) => Repository {
            root: root.to_path_buf(),
            name: repository_name(root),
            branch: "status unavailable".to_owned(),
            files: Vec::new(),
            error: Some(bound_message(&format!("{error:#}"), 160)),
        },
    }
}

fn run_status(root: &Path) -> Result<(String, Vec<FileChange>)> {
    let output = Command::new("git")
        .env("GIT_OPTIONAL_LOCKS", "0")
        .args([
            "-c",
            "core.fsmonitor=false",
            "-c",
            "core.quotepath=false",
            "-C",
        ])
        .arg(root)
        .args([
            "status",
            "--porcelain=v1",
            "-z",
            "--branch",
            "--untracked-files=normal",
            "--ignore-submodules=none",
        ])
        .output()
        .with_context(|| format!("could not run git in {}", root.display()))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        bail!("git status failed: {}", stderr.trim());
    }

    parse_porcelain(&output.stdout)
}

pub fn parse_porcelain(output: &[u8]) -> Result<(String, Vec<FileChange>)> {
    let records: Vec<&[u8]> = output
        .split(|byte| *byte == 0)
        .filter(|record| !record.is_empty())
        .collect();
    let mut branch = "HEAD".to_owned();
    let mut files = Vec::new();
    let mut index = 0;

    if records
        .first()
        .is_some_and(|record| record.starts_with(b"## "))
    {
        branch = parse_branch(&String::from_utf8_lossy(&records[0][3..]));
        index = 1;
    }

    while index < records.len() {
        let record = records[index];
        if record.len() < 3 || record[2] != b' ' {
            bail!("malformed git status record");
        }

        let index_status = record[0] as char;
        let worktree_status = record[1] as char;
        let path = sanitize(&String::from_utf8_lossy(&record[3..]));
        let renamed = matches!(index_status, 'R' | 'C') || matches!(worktree_status, 'R' | 'C');
        let original_path = if renamed {
            index += 1;
            let original = records
                .get(index)
                .context("rename record is missing its original path")?;
            Some(sanitize(&String::from_utf8_lossy(original)))
        } else {
            None
        };

        files.push(FileChange {
            path,
            original_path,
            index_status,
            worktree_status,
        });
        index += 1;
    }

    files.sort_by(|left, right| {
        left.path
            .to_lowercase()
            .cmp(&right.path.to_lowercase())
            .then_with(|| left.path.cmp(&right.path))
    });
    Ok((branch, files))
}

fn parse_branch(raw: &str) -> String {
    let raw = raw
        .strip_prefix("No commits yet on ")
        .or_else(|| raw.strip_prefix("Initial commit on "))
        .unwrap_or(raw);
    let branch = raw.split_once("...").map_or(raw, |(local, _)| local);
    let branch = branch.split_once(" [").map_or(branch, |(local, _)| local);

    match branch {
        "HEAD (no branch)" => "HEAD (detached)".to_owned(),
        value => sanitize(value.trim()),
    }
}

fn repository_name(root: &Path) -> String {
    root.file_name()
        .map(|name| sanitize(&name.to_string_lossy()))
        .filter(|name| !name.is_empty())
        .unwrap_or_else(|| sanitize(&root.to_string_lossy()))
}

pub fn sanitize(value: &str) -> String {
    value
        .chars()
        .map(|character| {
            if character.is_control() {
                '?'
            } else {
                character
            }
        })
        .collect()
}

fn bound_message(value: &str, limit: usize) -> String {
    let mut bounded: String = value.chars().take(limit).collect();
    if value.chars().count() > limit {
        bounded.push_str("...");
    }
    sanitize(&bounded)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_branch_and_common_status_records() {
        let input =
            b"## main...origin/main [ahead 2]\0 M package.json\0A  src/new.rs\0?? notes.txt\0";
        let (branch, files) = parse_porcelain(input).unwrap();

        assert_eq!(branch, "main");
        assert_eq!(files.len(), 3);
        assert_eq!(files[0].marker(), '?');
        assert_eq!(files[1].marker(), 'M');
        assert_eq!(files[2].marker(), 'A');
    }

    #[test]
    fn parses_nul_delimited_rename() {
        let input = b"## main\0R  new name.rs\0old name.rs\0";
        let (_, files) = parse_porcelain(input).unwrap();

        assert_eq!(files[0].path, "new name.rs");
        assert_eq!(files[0].original_path.as_deref(), Some("old name.rs"));
        assert_eq!(files[0].marker(), 'R');
    }

    #[test]
    fn marks_conflicts_as_unmerged() {
        let input = b"## main\0UU src/conflict.rs\0";
        let (_, files) = parse_porcelain(input).unwrap();
        assert_eq!(files[0].marker(), 'U');
    }
}
