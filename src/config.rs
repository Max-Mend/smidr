// SPDX-License-Identifier: MIT OR Apache-2.0
// Copyright (c) 2026 Max-Mend
// This file is part of smidr: https://github.com/Max-Mend/smidr

//! Types and (de)serialization for `smidr.toml`.
//!
//! This module owns the *shape* of the manifest file only — no filesystem
//! access beyond [`ManifestConfig::load`] and [`ManifestConfig::to_toml_string`],
//! and no process execution. Reading, validating, and acting on this data
//! is the job of [`crate::project`], [`crate::builder`], and
//! [`crate::toolchain`].

use crate::error::{BuildError, Result};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::path::Path;

/// The full, typed contents of a `smidr.toml` file.
#[derive(Deserialize, Serialize)]
pub struct ManifestConfig {
    pub project: ProjectSection,
    pub build: BuildSection,
    pub dependencies: BTreeMap<String, DependencySpec>,
}

/// The `[project]` section: identifying metadata for the project.
#[derive(Deserialize, Serialize)]
pub struct ProjectSection {
    pub name: String,
    pub version: String,
    pub authors: Vec<String>,
    pub authors_email: Vec<String>,
    pub description: Option<String>,
    pub license: Option<String>,
    pub c_standard: Option<String>,
}

/// The `[build]` section: compiler and flag settings used by
/// [`crate::builder::build_project`].
#[derive(Deserialize, Serialize)]
pub struct BuildSection {
    pub compiler: CompilerKind,
    pub warnings: WarningLevel,
    pub cflags: Vec<String>,
}

/// One entry under `[dependencies]` — describes where a dependency's
/// source comes from and how to build it.
///
/// Exactly one of `git` or `path` must be set; see
/// [`crate::resolver::resolve`] for how that's validated.
#[derive(Deserialize, Serialize)]
pub struct DependencySpec {
    /// Git repository URL. Mutually exclusive with `path`. Not yet
    /// implemented by [`crate::resolver`] — see the crate's roadmap.
    pub git: Option<String>,
    /// Path to a local copy of the dependency's source, relative to the
    /// project root. Mutually exclusive with `git`.
    pub path: Option<String>,
    /// Git tag, branch, or commit to check out. Only meaningful with `git`.
    pub tag: Option<String>,
    /// Which [`crate::toolchain::DepBuilder`] to use for this dependency.
    pub build_system: BuildSystemKind,
    /// Shell commands to run when `build_system = "custom"`. See
    /// [`crate::toolchain::CustomBuilder`].
    #[serde(default)]
    pub build_commands: Vec<String>,
    /// Additional include directories to add, relative to the dependency's
    /// install prefix. Used by [`crate::toolchain::CustomBuilder`].
    #[serde(default)]
    pub extra_includes: Vec<String>,
    /// Library names to link against (`-l<name>`), for build systems that
    /// can't report this automatically.
    #[serde(default)]
    pub libs: Vec<String>,
}

/// Which build system to use for a dependency. `Auto` probes the
/// dependency's source directory and picks the best match — see
/// [`crate::toolchain::resolve_builder`] for the detection order and
/// priority when more than one candidate is found.
#[derive(Debug, Deserialize, Serialize, Default, PartialEq, Eq)]
pub enum BuildSystemKind {
    #[default]
    Auto,
    Cmake,
    Meson,
    Make,
    Custom,
}

/// Which C compiler to use. `Auto` probes for a working compiler on
/// `PATH` — see `builder::compiler_binary` for the detection order.
#[derive(Deserialize, Serialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum CompilerKind {
    #[default]
    Auto,
    Gcc,
    Tcc,
    Clang,
}

/// Compiler warning level, translated into concrete flags by
/// `builder.rs` (`-Wall -Wextra`, plus `-Werror -Wpedantic` for `Strict`).
#[derive(Deserialize, Serialize, Default)]
pub enum WarningLevel {
    None,
    #[default]
    Standard,
    Strict,
}

impl ManifestConfig {
    /// Read and parse `smidr.toml` from `project_dir`.
    ///
    /// # Errors
    /// Returns [`BuildError::ManifestNotFound`] if no `smidr.toml` exists
    /// in `project_dir`, or [`BuildError::TomlDe`] if it exists but fails
    /// to parse.
    pub fn load(project_dir: &Path) -> Result<Self> {
        let manifest_path = project_dir.join("smidr.toml");

        if !manifest_path.exists() {
            return Err(BuildError::ManifestNotFound(manifest_path));
        }

        let text = std::fs::read_to_string(&manifest_path)?;
        let config: ManifestConfig = toml::from_str(&text)?;

        Ok(config)
    }

    /// Serialize this config back into a pretty-printed TOML string, for
    /// writing out a freshly scaffolded `smidr.toml` (see
    /// [`crate::project::Project::init`]).
    pub fn to_toml_string(&self) -> Result<String> {
        Ok(toml::to_string_pretty(self)?)
    }
}
