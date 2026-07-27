use std::ffi::OsStr;
use std::io::Read;
use std::path::Path;
use std::process::{Command, Stdio};
use std::thread;
use std::time::Duration;

#[cfg(unix)]
use std::os::unix::process::CommandExt;

use anyhow::{Context, Result, bail};
use wait_timeout::ChildExt;

use crate::model::{FileChange, Repository};

const GIT_STATUS_TIMEOUT: Duration = Duration::from_secs(10);

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
    run_status_with(root, OsStr::new("git"), GIT_STATUS_TIMEOUT)
}

fn run_status_with(
    root: &Path,
    git: &OsStr,
    timeout: Duration,
) -> Result<(String, Vec<FileChange>)> {
    let mut command = Command::new(git);
    command
        .env("GIT_OPTIONAL_LOCKS", "0")
        .env_remove("GIT_COMMON_DIR")
        .env_remove("GIT_DIR")
        .env_remove("GIT_INDEX_FILE")
        .env_remove("GIT_OBJECT_DIRECTORY")
        .env_remove("GIT_WORK_TREE")
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
            "--untracked-files=all",
            "--ignore-submodules=none",
        ])
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    #[cfg(unix)]
    command.process_group(0);

    let mut child = command
        .spawn()
        .with_context(|| format!("could not run git in {}", root.display()))?;
    let stdout = child
        .stdout
        .take()
        .context("could not capture git stdout")?;
    let stderr = child
        .stderr
        .take()
        .context("could not capture git stderr")?;
    let stdout_reader = thread::spawn(move || read_all(stdout));
    let stderr_reader = thread::spawn(move || read_all(stderr));

    let status = match child.wait_timeout(timeout) {
        Ok(Some(status)) => status,
        Ok(None) => {
            terminate(&mut child);
            let _ = join_reader(stdout_reader, "stdout");
            let _ = join_reader(stderr_reader, "stderr");
            bail!(
                "git status timed out after {} seconds",
                timeout.as_secs_f32()
            );
        }
        Err(error) => {
            terminate(&mut child);
            let _ = join_reader(stdout_reader, "stdout");
            let _ = join_reader(stderr_reader, "stderr");
            return Err(error).context("could not wait for git status");
        }
    };
    let stdout = join_reader(stdout_reader, "stdout")?;
    let stderr = join_reader(stderr_reader, "stderr")?;

    if !status.success() {
        let stderr = String::from_utf8_lossy(&stderr);
        bail!("git status failed: {}", stderr.trim());
    }

    parse_porcelain(&stdout)
}

fn terminate(child: &mut std::process::Child) {
    #[cfg(unix)]
    // SAFETY: the child was placed in a new process group whose id is its pid.
    unsafe {
        libc::killpg(child.id() as libc::pid_t, libc::SIGKILL);
    }
    let _ = child.kill();
    let _ = child.wait();
}

fn read_all(mut reader: impl Read) -> std::io::Result<Vec<u8>> {
    let mut output = Vec::new();
    reader.read_to_end(&mut output)?;
    Ok(output)
}

fn join_reader(
    reader: thread::JoinHandle<std::io::Result<Vec<u8>>>,
    stream: &str,
) -> Result<Vec<u8>> {
    reader
        .join()
        .map_err(|_| anyhow::anyhow!("git {stream} reader panicked"))?
        .with_context(|| format!("could not read git {stream}"))
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
    #[cfg(unix)]
    use std::fs;
    #[cfg(unix)]
    use std::os::unix::fs::PermissionsExt;
    #[cfg(unix)]
    use std::time::Instant;

    #[cfg(unix)]
    use tempfile::tempdir;

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

    #[cfg(unix)]
    #[test]
    fn terminates_a_git_process_that_exceeds_the_timeout() {
        let temp = tempdir().unwrap();
        let fake_git = temp.path().join("git");
        fs::write(&fake_git, "#!/bin/sh\nsleep 5 &\nwait\n").unwrap();
        let mut permissions = fs::metadata(&fake_git).unwrap().permissions();
        permissions.set_mode(0o755);
        fs::set_permissions(&fake_git, permissions).unwrap();
        let started = Instant::now();

        let error = run_status_with(
            temp.path(),
            fake_git.as_os_str(),
            Duration::from_millis(100),
        )
        .unwrap_err();

        assert!(error.to_string().contains("timed out"));
        assert!(started.elapsed() < Duration::from_secs(2));
    }
}
