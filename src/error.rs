// SPDX-License-Identifier: MIT OR Apache-2.0
// Copyright (c) 2026 Max-Mend
// This file is part of smidr: https://github.com/Max-Mend/smidr

//! The single error type for the entire `smidr` crate.
//!
//! Every fallible function in this crate returns [`Result<T>`], a thin
//! alias over [`std::result::Result`] with [`BuildError`] as the error
//! type. No module defines its own error enum - this keeps error handling
//! consistent across `config`, `project`, `builder`, `resolver`, and
//! `toolchain`, and lets every caller propagate failures with a single `?`.

use std::path::PathBuf;
use thiserror::Error;

/// Every way a `smidr` command can fail.
///
/// Variants are grouped by the module that produces them (see the comments
/// below); the grouping is informal but mirrors the crate's module layout.
#[derive(Error, Debug)]
pub enum BuildError {
    // ---- General / IO ----
    /// Wraps any [`std::io::Error`] (missing files, permission issues,
    /// failing to spawn a process, and so on).
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),

    /// A subprocess could not be started at all (distinct from a process
    /// that started but exited with a non-zero status - see
    /// [`BuildError::CommandFailed`]).
    #[error("Failed to run command: {0}")]
    Command(String),

    /// A subprocess ran but exited with a non-zero status code.
    #[error("Command `{cmd}` exited with code {code:?}")]
    CommandFailed { cmd: String, code: Option<i32> },

    // ---- Compilation / linking (builder.rs) ----
    /// Compiling a single `.c` file into an object file failed.
    /// The first field is the source file path, the second is the
    /// compiler's stderr output.
    #[error("Failed to compile {0}: {1}")]
    Compile(String, String),

    /// No usable C compiler could be found on the system (see
    /// `builder::compiler_binary`).
    #[error("Compiler '{0}' not found.")]
    CompilerNotFound(String),

    /// Linking the compiled object files into the final binary failed.
    #[error("Link error: {0}")]
    Link(String),

    /// `src/` exists but contains no `.c` files to compile.
    #[error("No .c files found in src/")]
    NoSourceFiles,

    /// `include/` exists but contains no `.h` files to compile.
    #[error("No .h files found in include/")]
    NoHeaderFiles,

    /// Cleaned target directory.
    #[error("Cleaned target directory: {0}")]
    Clean(String),

    /// A tool was not found.
    #[error("'{tool}' not found. {hint}")]
    ToolNotFound { tool: String, hint: String },

    // ---- Config (config.rs) ----
    /// Failed to serialize a [`crate::config::ManifestConfig`] back to TOML
    /// (used by `smidr new` when writing the generated `Smidr.toml`).
    #[error("Failed to serialize TOML: {0}")]
    TomlSer(#[from] toml::ser::Error),

    /// Failed to parse an existing `Smidr.toml` into a
    /// [`crate::config::ManifestConfig`].
    #[error("Failed to parse TOML: {0}")]
    TomlDe(#[from] toml::de::Error),

    /// `Smidr.toml` does not exist in the expected project directory.
    #[error("Smidr.toml not found in {0}")]
    ManifestNotFound(PathBuf),

    // ---- JSON (compile_db.rs) ----
    /// Failed to serialize the `compile_commands.json` entries.
    #[error("Failed to serialize JSON: {0}")]
    Json(#[from] serde_json::Error),

    // ---- Dependencies (resolver.rs / toolchain/*) ----
    /// A dependency's `build_system` value in `Smidr.toml` does not match
    /// any known [`crate::config::BuildSystemKind`].
    #[error("Unknown build system '{0}' for dependency '{1}'")]
    UnknownBuildSystem(String, String),

    /// `build_system = "auto"` was set for a dependency, but none of the
    /// supported build systems (CMake, Meson, Make) could be detected in
    /// its source directory.
    #[error("Could not detect build system for '{0}' - specify build_system in Smidr.toml")]
    BuildSystemDetectionFailed(String),

    /// A dependency-specific failure that doesn't fit the more specific
    /// variants above - used throughout `resolver.rs` and `toolchain/*`
    /// for things like an unreachable source path or a misconfigured
    /// `custom` build.
    #[error("Dependency '{name}' failed: {reason}")]
    Dependency { name: String, reason: String },

    // ---- Project (project.rs) ----
    /// `smidr new <name>` was called but a file or directory named `<name>`
    /// already exists.
    #[error("Project already exists: {0}")]
    ProjectAlreadyExists(PathBuf),

    /// The name passed to `smidr new` is empty, a path traversal sequence,
    /// or otherwise not a safe directory name.
    #[error("Invalid project name: {0}")]
    InvalidProjectName(String),
}

/// Convenience alias so the rest of the crate can write `Result<T>` instead
/// of `std::result::Result<T, BuildError>` everywhere.
pub type Result<T> = std::result::Result<T, BuildError>;
