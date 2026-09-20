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
        rev: Option<String>,
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
            rev,
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
                rev: rev.clone(),
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

fn clone_at_rev(url: &str, rev: &str, dest: &Path) -> Result<()> {
    run_git_clone(url, dest, &[]).map_err(|reason| BuildError::Dependency {
        name: dest.display().to_string(),
        reason: format!("failed to clone {}: {}", url, reason),
    })?;
    let output = Command::new("git")
        .args(["-C", &dest.to_string_lossy(), "checkout", rev])
        .output()
        .map_err(BuildError::Io)?;
    if !output.status.success() {
        return Err(BuildError::Dependency {
            name: dest.display().to_string(),
            reason: format!(
                "failed to checkout revision {}: {}",
                rev,
                String::from_utf8_lossy(&output.stderr).trim()
            ),
        });
    }
    Ok(())
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

/// The directory where cloned git dependencies are cached, shared across
/// every `smidr` project on the machine (analogous to Cargo's registry
/// cache) - `$XDG_CACHE_HOME/smidr/git`, falling back to
/// `$HOME/.cache/smidr/git`, or `.smidr-cache/git` under the current
/// directory if neither is set.
///
/// Keying the cache by `url` + resolved ref means the *same* dependency
/// pinned to the *same* tag/branch/rev is only ever fetched from the
/// network once, no matter how many projects (or how many times the same
/// project) depend on it - repeat resolves become a local directory copy
/// instead of a `git clone`.
fn global_cache_root() -> PathBuf {
    if let Ok(xdg) = std::env::var("XDG_CACHE_HOME") {
        return PathBuf::from(xdg).join("smidr").join("git");
    }
    if let Ok(home) = std::env::var("HOME") {
        return PathBuf::from(home).join(".cache").join("smidr").join("git");
    }
    PathBuf::from(".smidr-cache").join("git")
}

/// Turns a `url` + resolved ref into a filesystem-safe cache directory
/// name, e.g. `https://github.com/foo/bar` + `v1.2.3` ->
/// `github_com_foo_bar-v1_2_3`.
fn cache_key(url: &str, ref_to_clone: &str) -> String {
    let sanitize = |s: &str| -> String {
        s.chars()
            .map(|c| if c.is_ascii_alphanumeric() { c } else { '_' })
            .collect()
    };
    format!("{}-{}", sanitize(url), sanitize(ref_to_clone))
}

/// Recursively copies `src` into `dst`, creating directories as needed.
/// Used to materialize a project's dependency checkout from the shared
/// global cache without touching the network.
fn copy_dir_recursive(src: &Path, dst: &Path) -> std::io::Result<()> {
    std::fs::create_dir_all(dst)?;
    for entry in std::fs::read_dir(src)? {
        let entry = entry?;
        if entry.file_name() == ".git" {
            // Not needed in the copy - the working tree is already
            // checked out at the pinned ref, and for `rev` deps in
            // particular this can be the bulk of the cache's size
            // (no `--depth 1` there, since an arbitrary commit may not
            // be reachable from a shallow clone).
            continue;
        }
        let file_type = entry.file_type()?;
        let dst_path = dst.join(entry.file_name());
        if file_type.is_dir() {
            copy_dir_recursive(&entry.path(), &dst_path)?;
        } else {
            std::fs::copy(entry.path(), &dst_path)?;
        }
    }
    Ok(())
}

/// Resolve a git dependency into a local path, cloning it (at the pinned
/// tag, or the latest stable release if none is given) if not already cached.
///
/// The actual `git clone` happens at most once per `url` + resolved ref,
/// into a shared cache under [`global_cache_root`]; `dest` is then
/// populated as a local copy of that cache entry, so repeated resolves
/// (across builds, or across projects sharing a dependency) skip the
/// network entirely.
///
/// # Cache invalidation
/// The cache is keyed by `url` + resolved ref and is **never** refreshed
/// or invalidated automatically - only its *existence* is checked, not
/// its content. This is exact for `tag` and `rev` (both are immutable by
/// nature: the same tag or commit hash always means the same code), but
/// for `branch` it means the dependency is effectively pinned to
/// whatever commit was on that branch the first time it was resolved on
/// this machine, even after the branch moves forward upstream. To pick
/// up new commits on a tracked branch, clear the cached entry under
/// [`global_cache_root`] (or the whole cache directory) manually.
pub fn resolve_git(
    name: &str,
    url: &str,
    tag: &Option<String>,
    branch: &Option<String>,
    rev: &Option<String>,
    dest: &Path,
) -> Result<PathBuf> {
    let specified_count = [branch.is_some(), tag.is_some(), rev.is_some()]
        .iter()
        .filter(|&&specified| specified)
        .count();
    if specified_count > 1 {
        return Err(BuildError::Dependency {
            name: name.to_string(),
            reason: "only one of tag, branch, or rev may be specified".to_string(),
        });
    }

    let ref_to_clone = match (branch, tag, rev) {
        (Some(b), _, _) => b.clone(),
        (None, Some(t), _) => t.clone(),
        (None, None, Some(r)) => r.clone(),
        (None, None, None) => {
            let tags = list_tags(url)?;
            latest_stable_tag(tags).ok_or_else(|| BuildError::Dependency {
                name: name.to_string(),
                reason: "no stable release tags found; specify a tag or branch explicitly"
                    .to_string(),
            })?
        }
    };

    if dest.exists() {
        return Ok(dest.to_path_buf());
    }

    let cache_dir = global_cache_root().join(cache_key(url, &ref_to_clone));

    if cache_dir.exists() {
        println!(
            "Package '{}': using cached clone of '{}' (skipping network)",
            name, ref_to_clone
        );
    } else {
        println!("Package '{}': cloning at '{}'", name, ref_to_clone);
        // Clone into a temporary sibling first and rename into place once
        // complete, so a clone that fails or is interrupted partway
        // through can never leave a corrupt entry behind that a later,
        // successful run would mistake for a valid cache hit.
        let tmp_dir = global_cache_root().join(format!(
            "{}.tmp-{}",
            cache_key(url, &ref_to_clone),
            std::process::id()
        ));
        if let Some(parent) = tmp_dir.parent() {
            std::fs::create_dir_all(parent).map_err(BuildError::Io)?;
        }
        let clone_result = if branch.is_some() {
            clone_at_branch(url, &ref_to_clone, &tmp_dir)
        } else if tag.is_some() {
            clone_at_tag(url, &ref_to_clone, &tmp_dir)
        } else if rev.is_some() {
            clone_at_rev(url, &ref_to_clone, &tmp_dir)
        } else {
            // (None, None, None): ref_to_clone came from the latest stable
            // tag lookup above, so clone it the same way an explicit tag
            // would be cloned.
            clone_at_tag(url, &ref_to_clone, &tmp_dir)
        };

        if let Err(e) = clone_result {
            let _ = std::fs::remove_dir_all(&tmp_dir);
            return Err(e);
        }

        std::fs::rename(&tmp_dir, &cache_dir).map_err(BuildError::Io)?;
    }

    copy_dir_recursive(&cache_dir, dest).map_err(BuildError::Io)?;

    Ok(dest.to_path_buf())
}
