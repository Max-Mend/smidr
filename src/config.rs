// SPDX-License-Identifier: MIT OR Apache-2.0
// Copyright (c) 2026 Max-Mend
// This file is part of smidr: https://github.com/Max-Mend/smidr

//! Types and (de)serialization for `Smidr.toml`.
//!
//! This module owns the *shape* of the manifest file only - no filesystem
//! access beyond [`ManifestConfig::load`] and [`ManifestConfig::to_toml_string`],
//! and no process execution. Reading, validating, and acting on this data
//! is the job of [`crate::project`], [`crate::builder`], and
//! [`crate::toolchain`].

use crate::error::{BuildError, Result};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::path::Path;

/// The full, typed contents of a `Smidr.toml` file.
#[derive(Deserialize, Serialize)]
pub struct ManifestConfig {
    pub project: ProjectSection,
    pub build: BuildSection,
    pub dependencies: BTreeMap<String, DependencySpec>,
    /// Optional `[workspace]` section: lets this same `Smidr.toml` also
    /// act as the root of a multi-project workspace, listing paths to
    /// member projects. A `Smidr.toml` is a normal single project unless
    /// this is present - this project doesn't (yet) support Cargo-style
    /// "virtual" workspace-only manifests without a `[project]` section.
    #[serde(default)]
    pub workspace: Option<WorkspacesConfig>,
    #[serde(default, rename = "bin", skip_serializing_if = "Vec::is_empty")]
    pub extra_bins: Vec<BinTarget>,
}

/// The `[project]` section: identifying metadata for the project.
#[derive(Deserialize, Serialize)]
pub struct ProjectSection {
    pub name: String,
    pub version: String,
    #[serde(rename = "type")]
    pub project_type: ProjectType,
    #[serde(default)]
    pub language: Language,
    #[serde(default)]
    pub c_standard: CStandard,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cpp_standard: Option<CppStandard>,
    pub authors: Vec<String>,
    pub authors_email: Vec<String>,
    pub description: Option<String>,
    pub license: Option<String>,
    #[serde(default)]
    pub output_name: Option<String>,
}

/// The type of the project, either a binary, static library, or shared library.
#[derive(Debug, Clone, Deserialize, Serialize, Default, PartialEq, Eq, clap::ValueEnum)]
pub enum ProjectType {
    #[default]
    #[serde(rename = "bin")]
    #[value(name = "bin")]
    Binary,
    #[serde(rename = "static")]
    #[value(name = "static")]
    StaticLibrary,
    #[serde(rename = "dynamic")]
    #[value(name = "dynamic")]
    SharedLibrary,
}

#[derive(Debug, Clone, clap::ValueEnum, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Language {
    #[default]
    C,
    Cpp,
}

#[derive(Debug, Clone, clap::ValueEnum, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum CStandard {
    C89,
    C90,
    C99,
    C11,
    #[default]
    C17,
    C23,
    Gnu99,
    Gnu11,
    Gnu17,
}

#[derive(Debug, Clone, clap::ValueEnum, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum CppStandard {
    Cpp98,
    Cpp03,
    Cpp11,
    Cpp14,
    Cpp17,
    #[default]
    Cpp20,
    Cpp23,
    Cpp26,
}

/// `[workspace]` section - a list of paths to member projects, each with
/// its own `Smidr.toml`.
#[derive(Debug, Deserialize, Serialize)]
pub struct WorkspacesConfig {
    pub members: Vec<String>,
}

/// The `[build]` section: compiler and flag settings used by
/// [`crate::builder::build_project`].
#[derive(Deserialize, Serialize)]
pub struct BuildSection {
    pub compiler: CompilerKind,
    pub warnings: WarningLevel,
    pub cflags: Vec<String>,
    pub libs: Vec<String>,
}

/// One entry under `[dependencies]` - describes where a dependency's
/// source comes from and how to build it.
///
/// Exactly one of `git` or `path` must be set; see
/// [`crate::resolver::resolve`] for how that's validated.
#[derive(Deserialize, Serialize)]
pub struct DependencySpec {
    /// Git repository URL. Mutually exclusive with `path`. Not yet
    /// implemented by [`crate::resolver`] - see the crate's roadmap.
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

/// The `[profile]` section: compiler and optimization settings.
#[derive(Deserialize, Serialize, Default)]
pub struct ProfileSection {
    #[serde(default)]
    pub opt_level: OptLevel,
    #[serde(default)]
    pub debug_symbols: bool,
}

/// Optimization level for the build.
#[derive(Deserialize, Serialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum OptLevel {
    #[default]
    None,   // -O0
    Speed,  // -O2
    Size,   // -Os
    Max,    // -O3
}

#[derive(Deserialize, Serialize)]
pub struct BinTarget {
    pub name: String,
    pub path: String,
}

/// Which build system to use for a dependency. `Auto` probes the
/// dependency's source directory and picks the best match - see
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
/// `PATH` - see `builder::compiler_binary` for the detection order.
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
    /// Read and parse `Smidr.toml` from `project_dir`.
    ///
    /// # Errors
    /// Returns [`BuildError::ManifestNotFound`] if no `Smidr.toml` exists
    /// in `project_dir`, or [`BuildError::TomlDe`] if it exists but fails
    /// to parse.
    pub fn load(project_dir: &Path) -> Result<Self> {
        let manifest_path = project_dir.join("Smidr.toml");

        if !manifest_path.exists() {
            return Err(BuildError::ManifestNotFound(manifest_path));
        }

        let text = std::fs::read_to_string(&manifest_path)?;
        let config: ManifestConfig = toml::from_str(&text)?;

        Ok(config)
    }

    /// Serialize this config back into a pretty-printed TOML string, for
    /// writing out a freshly scaffolded `Smidr.toml` 
    /// (see [`crate::project::Project::init`]).
    pub fn to_toml_string(&self) -> Result<String> {
        Ok(toml::to_string_pretty(self)?)
    }
}

impl ProjectSection {
    /// Returns the explicit `output_name` if configured, falling back to the project `name`.
    pub fn output_name(&self) -> &str {
        self.output_name.as_deref().unwrap_or(&self.name)
    }
}

// Implementation of the Display trait for CStandard, used for printing the CStandard enum to a string.
impl std::fmt::Display for CStandard {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let s = match self {
            CStandard::C89 => "c89",
            CStandard::C90 => "c90",
            CStandard::C99 => "c99",
            CStandard::C11 => "c11",
            CStandard::C17 => "c17",
            CStandard::C23 => "c23",
            CStandard::Gnu99 => "gnu99",
            CStandard::Gnu11 => "gnu11",
            CStandard::Gnu17 => "gnu17",
        };
        f.write_str(s)
    }
}

// Implementation of the Display trait for CppStandard, used for printing the CppStandard enum to a string.
impl std::fmt::Display for CppStandard {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let s = match self {
            CppStandard::Cpp98 => "c++98",
            CppStandard::Cpp03 => "c++03",
            CppStandard::Cpp11 => "c++11",
            CppStandard::Cpp14 => "c++14",
            CppStandard::Cpp17 => "c++17",
            CppStandard::Cpp20 => "c++20",
            CppStandard::Cpp23 => "c++23",
            CppStandard::Cpp26 => "c++26",
        };
        f.write_str(s)
    }
}
