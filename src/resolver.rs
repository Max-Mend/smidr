// SPDX-License-Identifier: MIT OR Apache-2.0
// Copyright (c) 2026 Max-Mend
// This file is part of smidr: https://github.com/Max-Mend/smidr

//! Resolves where a dependency's source actually lives on disk.
//!
//! This module only answers "where is the source?" - it has no opinion
//! on how that source gets built (that's [`crate::toolchain`]). Currently
//! only local `path =` dependencies are implemented; `git =` is parsed
//! and validated but not yet fetched.

use crate::config::DependencySpec;
use crate::error::{BuildError, Result};
use std::path::{Path, PathBuf};

/// Where a dependency's source was found.
pub enum SourceLocation {
    /// A local path, already resolved to an absolute, existing directory.
    Path(PathBuf),
    /// A git repository to clone. Not yet consumed anywhere - see
    /// [`resolve`].
    Git { url: String, tag: String },
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
/// are set, if a `path` dependency doesn't exist on disk, or if a `git`
/// dependency is specified (not yet supported).
pub fn resolve(name: &str, spec: &DependencySpec, project_root: &Path) -> Result<SourceLocation> {
    match (&spec.git, &spec.path) {
        (None, None) => Err(BuildError::Dependency {
            name: name.to_string(),
            reason: "no source specified: specify git or path in smidr.toml".to_string(),
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

        (Some(_git_url), None) => Err(BuildError::Dependency {
            name: name.to_string(),
            reason: "git repositories are not supported yet".to_string(),
        }),
    }
}
