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
use std::path::{Path, PathBuf};
use std::process::Command;

/// Where a dependency's source was found.
pub enum SourceLocation {
    Path(PathBuf),
    Git { url: String, tag: Option<String> },
    /// A system library, resolved via local search or `pkg-config`.
    /// Not yet consumed anywhere - see [`resolve`].
    System { version: String },
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
    let cflags_out = Command::new("pkg-config").args(["--cflags", name]).output().ok()?;
    let libs_out = Command::new("pkg-config").args(["--libs", name]).output().ok()?;

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
        DependencySpec::Version(version) => {
            Ok(SourceLocation::System { version: version.clone() })
        }

        DependencySpec::Detailed { git, path, tag, .. } => {
            match (git, path) {
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
                }),
            }
        }
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
    let status = Command::new("git")
        .args(["clone", "--branch", tag, "--depth", "1", url])
        .arg(dest)
        .status()
        .map_err(BuildError::Io)?;

    if !status.success() {
        return Err(BuildError::Dependency {
            name: dest.display().to_string(),
            reason: format!("failed to clone {} at tag {}", url, tag),
        });
    }
    Ok(())
}

/// Resolve a git dependency into a local path, cloning it (at the pinned
/// tag, or the latest stable release if none is given) if not already cached.
pub fn resolve_git(name: &str, url: &str, tag: &Option<String>, dest: &Path) -> Result<PathBuf> {
    let resolved_tag = match tag {
        Some(t) => t.clone(),
        None => {
            let tags = list_tags(url)?;
            latest_stable_tag(tags).ok_or_else(|| BuildError::Dependency {
                name: name.to_string(),
                reason: "no stable release tags found; specify a tag explicitly".to_string(),
            })?
        }
    };

    if !dest.exists() {
        println!("Package '{}': cloning at tag '{}'", name, resolved_tag);
        clone_at_tag(url, &resolved_tag, dest)?;
    }

    Ok(dest.to_path_buf())
}