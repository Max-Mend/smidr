// SPDX-License-Identifier: MIT OR Apache-2.0
// Copyright (c) 2026 Max-Mend
// This file is part of smidr: https://github.com/Max-Mend/smidr

//! Resolves where a dependency's source actually lives on disk.
//!
//! This module only answers "where is the source?" - it has no opinion
//! on how that source gets built (that's [`crate::toolchain`]).
//! `path` dependencies are resolved directly; `git` dependencies are
//! cloned (at a pinned tag, or the latest stable release if none is
//! given) before being resolved the same way as `path`.

use crate::config::DependencySpec;
use crate::error::{BuildError, Result};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

/// Where a dependency's source was found.
pub enum SourceLocation {
    Path(PathBuf),
    Git {
        url: String,
        tag: Option<String>,
        branch: Option<String>,
    },
    /// A system library, resolved via local search or `pkg-config`.
    /// Not yet consumed anywhere - see [`resolve`].
    System,
}

/// A system library found via a local search or `pkg-config`.
pub struct SystemLibInfo {
    pub cflags: Vec<String>,
    pub libs: Vec<String>,
}

fn check_known_paths(name: &str) -> Option<SystemLibInfo> {
    let search_dirs = ["/usr/include", "/usr/local/include"];
    for dir in search_dirs {
        let header = Path::new(dir).join(format!("{}.h", name));
        if header.exists() {
            return Some(SystemLibInfo {
                cflags: vec![format!("-I{}", dir)],
                libs: vec![format!("-l{}", name)],
            });
        }
    }
    None
}

fn try_pkg_config(name: &str) -> Option<SystemLibInfo> {
    let cflags_out = Command::new("pkg-config")
        .args(["--cflags", name])
        .output()
        .ok()?;
    let libs_out = Command::new("pkg-config")
        .args(["--libs", name])
        .output()
        .ok()?;

    if !cflags_out.status.success() || !libs_out.status.success() {
        return None;
    }

    Some(SystemLibInfo {
        cflags: parse_flags(&cflags_out.stdout),
        libs: parse_flags(&libs_out.stdout),
    })
}

fn parse_flags(bytes: &[u8]) -> Vec<String> {
    String::from_utf8_lossy(bytes)
        .split_whitespace()
        .map(String::from)
        .collect()
}

pub fn resolve_system_lib(name: &str) -> Result<SystemLibInfo> {
    if let Some(found) = check_known_paths(name) {
        return Ok(found);
    }
    if let Some(pkg) = try_pkg_config(name) {
        return Ok(pkg);
    }
    Err(BuildError::Dependency {
        name: name.to_string(),
        reason: "not found locally or via pkg-config".to_string(),
    })
}

/// Resolve a single dependency's `git`/`path` spec into a
/// [`SourceLocation`].
///
/// Exactly one of `spec.git` or `spec.path` must be set; both missing or
/// both present are treated as configuration errors rather than silently
/// picking one.
///
/// # Errors
/// Returns [`BuildError::Dependency`] if neither or both of `git`/`path`
/// are set, if a `path` dependency does not exist on disk, or if a `git`
/// dependency is specified (not yet supported).
pub fn resolve(name: &str, spec: &DependencySpec, project_root: &Path) -> Result<SourceLocation> {
    match spec {
        DependencySpec::Version(_version) => Ok(SourceLocation::System),

        DependencySpec::Detailed {
            git,
            path,
            tag,
            branch,
            ..
        } => match (git, path) {
            (None, None) => Err(BuildError::Dependency {
                name: name.to_string(),
                reason: "no source specified: specify git or path in Smidr.toml".to_string(),
            }),

            (Some(_), Some(_)) => Err(BuildError::Dependency {
                name: name.to_string(),
                reason: "both git and path specified - ambiguous".to_string(),
            }),

            (None, Some(local_path)) => {
                let full = project_root.join(local_path);
                if !full.exists() {
                    return Err(BuildError::Dependency {
                        name: name.to_string(),
                        reason: format!("path not found: {}", full.display()),
                    });
                }
                Ok(SourceLocation::Path(full))
            }

            (Some(git_url), None) => Ok(SourceLocation::Git {
                url: git_url.clone(),
                tag: tag.clone(),
                branch: branch.clone(),
            }),
        },
    }
}

fn list_tags(url: &str) -> Result<Vec<String>> {
    let output = Command::new("git")
        .args(["ls-remote", "--tags", "--refs", url])
        .output()
        .map_err(BuildError::Io)?;

    if !output.status.success() {
        return Err(BuildError::Dependency {
            name: url.to_string(),
            reason: format!(
                "failed to query tags: {}",
                String::from_utf8_lossy(&output.stderr).trim()
            ),
        });
    }

    Ok(String::from_utf8_lossy(&output.stdout)
        .lines()
        .filter_map(|line| line.split('/').last().map(String::from))
        .collect())
}

fn latest_stable_tag(tags: Vec<String>) -> Option<String> {
    tags.into_iter()
        .filter_map(|tag| {
            let clean = tag.trim_start_matches('v');
            semver::Version::parse(clean).ok().map(|v| (tag, v))
        })
        .filter(|(_, v)| v.pre.is_empty())
        .max_by(|(_, a), (_, b)| a.cmp(b))
        .map(|(tag, _)| tag)
}

fn clone_at_tag(url: &str, tag: &str, dest: &Path) -> Result<()> {
    run_git_clone(url, dest, &["--branch", tag, "--depth", "1"]).map_err(|reason| {
        BuildError::Dependency {
            name: dest.display().to_string(),
            reason: format!("failed to clone {} at tag {}: {}", url, tag, reason),
        }
    })
}

fn clone_at_branch(url: &str, branch: &str, dest: &Path) -> Result<()> {
    run_git_clone(url, dest, &["--branch", branch, "--depth", "1"]).map_err(|reason| {
        BuildError::Dependency {
            name: dest.display().to_string(),
            reason: format!("failed to clone {} at branch {}: {}", url, branch, reason),
        }
    })
}

/// Runs `git clone` with the given extra args, streaming stderr to the
/// user live (so long clones are not a silent black box) while also
/// capturing it, so a failure can report *why* it failed.
///
/// git's clone progress meter ("Receiving objects: 45%...") updates in
/// place using `\r`, not `\n`, and git only emits it at all when it
/// thinks it's talking to a terminal, which it is not once stderr is
/// piped. `--progress` forces git to emit the meter anyway, and reading
/// raw byte chunks (instead of `BufRead::lines`, which waits for `\n`
/// and would otherwise sit silent until the whole clone finished)
/// preserves the `\r` redraws so the live output looks the same as a
/// bare `git clone` in the terminal.
fn run_git_clone(url: &str, dest: &Path, extra_args: &[&str]) -> std::result::Result<(), String> {
    let mut child = Command::new("git")
        .arg("clone")
        .arg("--progress")
        .args(extra_args)
        .arg(url)
        .arg(dest)
        .stdout(Stdio::inherit())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|e| e.to_string())?;

    let mut stderr = child.stderr.take().expect("stderr was piped");
    let mut captured = Vec::new();
    let mut buf = [0u8; 4096];
    loop {
        let n = stderr.read(&mut buf).map_err(|e| e.to_string())?;
        if n == 0 {
            break;
        }
        let chunk = &buf[..n];
        // Passthrough as-is: this keeps \r redraws intact so the
        // progress meter animates the same way it would unpiped.
        std::io::stderr()
            .write_all(chunk)
            .map_err(|e| e.to_string())?;
        captured.extend_from_slice(chunk);
    }

    let status = child.wait().map_err(|e| e.to_string())?;
    if !status.success() {
        // Progress lines are separated by \r rather than \n, so pull the
        // last non-empty segment on either separator for the error text.
        let text = String::from_utf8_lossy(&captured).into_owned();
        let last_line = text
            .split(['\r', '\n'])
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .last()
            .unwrap_or("")
            .to_string();
        return Err(last_line);
    }
    Ok(())
}

/// Resolve a git dependency into a local path, cloning it (at the pinned
/// tag, or the latest stable release if none is given) if not already cached.
pub fn resolve_git(
    name: &str,
    url: &str,
    tag: &Option<String>,
    branch: &Option<String>,
    dest: &Path,
) -> Result<PathBuf> {
    let ref_to_clone = match (branch, tag) {
        (Some(b), _) => b.clone(),
        (None, Some(t)) => t.clone(),
        (None, None) => {
            let tags = list_tags(url)?;
            latest_stable_tag(tags).ok_or_else(|| BuildError::Dependency {
                name: name.to_string(),
                reason: "no stable release tags found; specify a tag or branch explicitly"
                    .to_string(),
            })?
        }
    };

    if !dest.exists() {
        println!("Package '{}': cloning at '{}'", name, ref_to_clone);
        if branch.is_some() {
            clone_at_branch(url, &ref_to_clone, dest)?;
        } else {
            clone_at_tag(url, &ref_to_clone, dest)?;
        }
    }

    Ok(dest.to_path_buf())
}
