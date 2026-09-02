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
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub project: Option<ProjectSection>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub build: Option<BuildSection>,
    #[serde(default)]
    pub dependencies: BTreeMap<String, DependencySpec>,
    /// Optional `[workspace]` section: lets this same `Smidr.toml` also
    /// act as the root of a multi-project workspace, listing paths to
    /// member projects. A `Smidr.toml` is a normal single project unless
    /// this is present - this project doesn't (yet) support Cargo-style
    /// "virtual" workspace-only manifests without a `[project]` section.
    #[serde(default)]
    pub workspace: Option<WorkspacesConfig>,
    #[serde(default, skip_serializing_if = "ProfilesSection::is_empty")]
    pub profile: ProfilesSection,
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
    pub cflags: Vec<String>,
    pub libs: Vec<String>,
    #[serde(default)]
    pub linker_flags: Vec<String>,
}

/// One entry under `[dependencies]` - describes where a dependency's
/// source comes from and how to build it.
///
/// Exactly one of `git` or `path` must be set; see
/// [`crate::resolver::resolve`] for how that's validated.
#[derive(Deserialize, Serialize, Clone)]
#[serde(untagged)]
pub enum DependencySpec {
    /// A version string for a system library, e.g. `zlib = "1.3"`.
    /// Resolved via a local search, then `pkg-config`.
    Version(String),
    /// A `git`, `path`, or build-system-configured dependency.
    Detailed {
        git: Option<String>,
        path: Option<String>,
        tag: Option<String>,
        #[serde(default)]
        build_system: BuildSystemKind,
        #[serde(default)]
        build_commands: Vec<String>,
        #[serde(default)]
        extra_includes: Vec<String>,
        #[serde(default)]
        libs: Vec<String>,
    },
}

/// The `[profile]` section in Smidr.toml. (All fields optional)
#[derive(Debug, Clone, Deserialize, Serialize, Default, PartialEq, Eq)]
pub struct ProfilesSection {
    pub debug: Option<ProfileSection>,
    pub release: Option<ProfileSection>,
}

/// The `[profile.debug]` and `[profile.release]` sections in Smidr.toml. (All fields optional)
#[derive(Debug, Clone, Deserialize, Serialize, Default, PartialEq, Eq)]
pub struct ProfileSection {
    pub opt_level: Option<OptLevel>,
    pub warnings: Option<WarningLevel>,
    pub debug_symbols: Option<bool>,
    pub lto: Option<bool>,
    pub strip: Option<bool>,
}

/// The effective, fully-resolved settings for a build profile - every
/// field filled in, either from the user's `[profile.debug]`/
/// `[profile.release]` in `Smidr.toml`, or from the profile's built-in
/// defaults. This is what `builder::build_project` actually reads;
/// [`ProfileSection`] (all-`Option`) is only the on-disk, possibly-partial
/// representation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ResolvedProfile {
    pub opt_level: OptLevel,
    pub warnings: WarningLevel,
    pub debug_symbols: bool,
    pub lto: bool,
    pub strip: bool,
}

/// Optimization level for the build.
#[derive(Debug, Clone, Copy, Deserialize, Serialize, Default, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum OptLevel {
    #[default]
    None,   // -O0
    Speed,  // -O2
    Size,   // -Os
    Max,    // -O3
}

/// Compiler warning level, translated into concrete flags by
/// `builder.rs` (`-Wall -Wextra`, plus `-Werror -Wpedantic` for `Strict`).
#[derive(Debug, Clone, Copy, Deserialize, Serialize, Default, PartialEq, Eq)]
pub enum WarningLevel {
    None,
    #[default]
    Standard,
    Strict,
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
#[derive(Debug, Deserialize, Serialize, Default, PartialEq, Eq, Clone)]
#[serde(rename_all = "lowercase")]
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

    /// Returns the effective release profile, using defaults if not specified.
    pub fn get_release_profile(&self) -> ResolvedProfile {
        let user_profile = self.profile.release.as_ref();
        ResolvedProfile {
            opt_level: user_profile
                .and_then(|p| p.opt_level)
                .unwrap_or(OptLevel::Max),
            warnings: user_profile
                .and_then(|p| p.warnings)
                .unwrap_or(WarningLevel::Strict),
            debug_symbols: user_profile
                .and_then(|p| p.debug_symbols)
                .unwrap_or(false),
            lto: user_profile.and_then(|p| p.lto).unwrap_or(true),
            strip: user_profile.and_then(|p| p.strip).unwrap_or(false),
        }
    }

    /// Returns the effective debug profile, using defaults if not specified.
    pub fn get_debug_profile(&self) -> ResolvedProfile {
        let user_profile = self.profile.debug.as_ref();
        ResolvedProfile {
            opt_level: user_profile
                .and_then(|p| p.opt_level)
                .unwrap_or(OptLevel::None),
            warnings: user_profile
                .and_then(|p| p.warnings)
                .unwrap_or(WarningLevel::Standard),
            debug_symbols: user_profile
                .and_then(|p| p.debug_symbols)
                .unwrap_or(true),
            lto: user_profile.and_then(|p| p.lto).unwrap_or(false),
            strip: user_profile.and_then(|p| p.strip).unwrap_or(false),
        }
    }
}

impl ProjectSection {
    /// Returns the explicit `output_name` if configured, falling back to the project `name`.
    pub fn output_name(&self) -> &str {
        self.output_name.as_deref().unwrap_or(&self.name)
    }
}

impl ProfilesSection {
    /// Returns `true` if no profile settings are specified (both `debug` and `release` are `None`).
    ///
    /// This is used by `toml::to_string_pretty` to decide whether to serialize the `profile` field
    pub fn is_empty(&self) -> bool {
        self.debug.is_none() && self.release.is_none()
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