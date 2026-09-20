//! Local git metadata for the workspace details pane.
//! Mirrors `HerdrService.GitRepositoryInfo.parse` / `local(at:)`.
//! Do not canonicalize: that would resolve symlinks unlike Swift `standardizedFileURL`.

use std::path::{Component, Path, PathBuf};
use std::process::{Command, Stdio};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct GitRepositoryInfo {
    pub repository_name: String,
    pub branch: Option<String>,
    pub is_worktree: bool,
}

impl GitRepositoryInfo {
    pub fn parse(output: &str) -> Option<Self> {
        let lines: Vec<&str> = output.lines().filter(|line| !line.is_empty()).collect();
        if lines.len() < 4 {
            return None;
        }
        let root = normalize(Path::new(lines[0]));
        if root.as_os_str().is_empty() {
            return None;
        }
        let git_directory = resolve(lines[1], &root);
        let common_directory = resolve(lines[2], &root);
        let branch = lines[3].trim();
        let repository_root = if common_directory.file_name().is_some_and(|n| n == ".git") {
            common_directory
                .parent()
                .map(Path::to_path_buf)
                .unwrap_or(root.clone())
        } else {
            root.clone()
        };
        let repository_name = repository_root
            .file_name()
            .and_then(|n| n.to_str())
            .filter(|n| !n.is_empty())?
            .to_string();
        Some(Self {
            repository_name,
            branch: if branch.is_empty() || branch == "HEAD" {
                None
            } else {
                Some(branch.to_string())
            },
            is_worktree: git_directory != common_directory,
        })
    }
}

pub fn probe_local(cwd: &str) -> Option<GitRepositoryInfo> {
    let cwd = cwd.trim();
    if cwd.is_empty() {
        return None;
    }
    let mut command = if Path::new("/usr/bin/git").is_file() {
        Command::new("/usr/bin/git")
    } else {
        Command::new("git")
    };
    command
        .args([
            "-C",
            cwd,
            "rev-parse",
            "--path-format=absolute",
            "--show-toplevel",
            "--git-dir",
            "--git-common-dir",
            "--abbrev-ref",
            "HEAD",
        ])
        .stdin(Stdio::null())
        .stderr(Stdio::null());
    let output = command.output().ok()?;
    if !output.status.success() {
        return None;
    }
    GitRepositoryInfo::parse(std::str::from_utf8(&output.stdout).ok()?)
}

fn resolve(path: &str, root: &Path) -> PathBuf {
    let joined = if path.starts_with('/') {
        PathBuf::from(path)
    } else {
        root.join(path)
    };
    normalize(&joined)
}

fn normalize(path: &Path) -> PathBuf {
    let mut out = PathBuf::new();
    for component in path.components() {
        match component {
            Component::CurDir => {}
            Component::ParentDir => {
                out.pop();
            }
            other => out.push(other.as_os_str()),
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_distinguishes_linked_worktree_and_branch() {
        let output = "/Users/me/project/wt\n/Users/me/project/.git/worktrees/wt\n/Users/me/project/.git\nfeature/ui\n";
        let info = GitRepositoryInfo::parse(output).expect("worktree sample");
        assert_eq!(info.repository_name, "project");
        assert_eq!(info.branch.as_deref(), Some("feature/ui"));
        assert!(info.is_worktree);
    }

    #[test]
    fn parse_rejects_missing_git_metadata() {
        assert_eq!(GitRepositoryInfo::parse("not-a-git-directory\n"), None);
    }

    #[test]
    fn parse_relative_git_dir_and_detached_head() {
        let output = "/Users/me/project\n.git\n.git\nHEAD\n";
        let info = GitRepositoryInfo::parse(output).expect("relative git-dir");
        assert_eq!(info.repository_name, "project");
        assert_eq!(info.branch, None);
        assert!(!info.is_worktree);
    }
}
