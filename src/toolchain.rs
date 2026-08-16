// SPDX-License-Identifier: MIT OR Apache-2.0
// Copyright (c) 2026 Max-Mend
// This file is part of smidr: https://github.com/Max-Mend/smidr

//! Build-system abstraction for dependencies.
//!
//! Each dependency in `[dependencies]` needs to be built with *some*
//! external build system — CMake, Meson, a plain Makefile, or a
//! user-defined shell script. This module defines the one contract
//! ([`DepBuilder`]) that every such system implements, and picks the
//! right implementation for a given dependency ([`resolve_builder`]).
//! Callers outside this module only ever see a `Box<dyn DepBuilder>` and
//! a [`BuildOutput`] — never `cmake`, `meson`, or `make` directly.
//!
//! Concrete implementations live in the `toolchain/` submodules:
//! [`CMakeBuilder`], [`MesonBuilder`], [`MakeBuilder`], [`CustomBuilder`].

mod cmake;
mod custom;
mod make;
mod meson;

use crate::config::{BuildSystemKind, DependencySpec};
use crate::error::{BuildError, Result};
use std::path::{Path, PathBuf};
use std::process::Command;

pub use cmake::CMakeBuilder;
pub use custom::CustomBuilder;
pub use make::MakeBuilder;
pub use meson::MesonBuilder;

/// What building a dependency produces: everything [`crate::builder`]
/// needs to compile and link the user's project against it.
#[derive(Debug, Clone, Default)]
pub struct BuildOutput {
    /// Directories to add as `-I` flags.
    pub include_dirs: Vec<PathBuf>,
    /// Directories to add as `-L` flags.
    pub lib_dirs: Vec<PathBuf>,
    /// Library names to add as `-l` flags. Only populated for
    /// [`CustomBuilder`] currently — CMake/Meson/Make builders leave this
    /// empty (see the crate's roadmap for `pkg-config` resolution).
    pub libs: Vec<String>,
}

/// The contract every dependency build system implements.
///
/// Implementors are picked dynamically at runtime via
/// [`resolve_builder`], so callers work with `Box<dyn DepBuilder>` rather
/// than a concrete type.
pub trait DepBuilder {
    /// Whether this build system looks usable for `src_dir` — typically
    /// implemented as "does the expected manifest file exist here"
    /// (e.g. `CMakeLists.txt` for CMake). Used by `BuildSystemKind::Auto`
    /// detection; not called for an explicitly configured build system.
    fn detect(src_dir: &Path) -> bool
    where
        Self: Sized;

    /// A short, human-readable name for this build system ("cmake",
    /// "meson", "make", "custom"), printed to the user so the choice made
    /// by [`resolve_builder`] is never a silent black box.
    fn name(&self) -> &'static str;

    /// Build and install the dependency into `prefix`, returning the
    /// resulting include/lib paths.
    fn build(&self, src_dir: &Path, prefix: &Path) -> Result<BuildOutput>;
}

/// Pick and construct the right [`DepBuilder`] for a dependency.
///
/// For `BuildSystemKind::Auto`, probes `src_dir` for each supported build
/// system and prefers, in order: CMake, Meson, Make (CMake and Meson both
/// guarantee a well-defined install prefix; a bare Makefile does not —
/// see [`make`]). If more than one candidate is detected, the choice is
/// still made automatically, but is printed to the user so it's never a
/// surprise. Always prints which build system was ultimately used.
///
/// # Errors
/// Returns [`BuildError::BuildSystemDetectionFailed`] if `Auto` is set
/// and no supported build system is detected in `src_dir`.
pub fn resolve_builder(
    dep_name: &str,
    kind: &BuildSystemKind,
    src_dir: &Path,
    spec: &DependencySpec,
) -> Result<Box<dyn DepBuilder>> {
    let builder: Box<dyn DepBuilder> = match kind {
        BuildSystemKind::Cmake => Box::new(CMakeBuilder),
        BuildSystemKind::Meson => Box::new(MesonBuilder),
        BuildSystemKind::Make => Box::new(MakeBuilder),
        BuildSystemKind::Custom => Box::new(CustomBuilder::new(spec)),
        BuildSystemKind::Auto => {
            let candidates = detect_available(src_dir);
            match candidates.as_slice() {
                [] => {
                    return Err(BuildError::BuildSystemDetectionFailed(
                        src_dir.display().to_string(),
                    ))
                }
                [only] => build_of(only, spec),
                multiple => {
                    // Multiple build systems detected at once (e.g. both a
                    // CMakeLists.txt and a Makefile) — pick by priority,
                    // but the user should see this, not have to guess why
                    // CMake was chosen.
                    eprintln!(
                        "Error: '{}': found multiple build systems ({}), choosing '{}'. \
                         To change - specify build_system in smidr.toml.",
                        dep_name,
                        multiple
                            .iter()
                            .map(|k| format!("{:?}", k))
                            .collect::<Vec<_>>()
                            .join(", "),
                        format!("{:?}", multiple[0])
                    );
                    build_of(&multiple[0], spec)
                }
            }
        }
    };

    println!("Package '{}': building with {}", dep_name, builder.name());
    Ok(builder)
}

/// All build systems whose manifest file is present in `src_dir`, used
/// for `BuildSystemKind::Auto` detection.
fn detect_available(src_dir: &Path) -> Vec<BuildSystemKind> {
    let mut found = Vec::new();
    if CMakeBuilder::detect(src_dir) {
        found.push(BuildSystemKind::Cmake);
    }
    if MesonBuilder::detect(src_dir) {
        found.push(BuildSystemKind::Meson);
    }
    if MakeBuilder::detect(src_dir) {
        found.push(BuildSystemKind::Make);
    }
    found
}

/// Construct the [`DepBuilder`] for an already-decided, non-`Auto` kind.
fn build_of(kind: &BuildSystemKind, spec: &DependencySpec) -> Box<dyn DepBuilder> {
    match kind {
        BuildSystemKind::Cmake => Box::new(CMakeBuilder),
        BuildSystemKind::Meson => Box::new(MesonBuilder),
        BuildSystemKind::Make => Box::new(MakeBuilder),
        BuildSystemKind::Custom => Box::new(CustomBuilder::new(spec)),
        BuildSystemKind::Auto => unreachable!("Auto not resolved in build_of"),
    }
}

/// Run a command to completion, printing it first and converting a
/// failure to start or a non-zero exit code into a [`BuildError`].
///
/// Private to this module tree — accessible from the `toolchain/*`
/// submodules via `super::run`, since child modules can see their
/// ancestors' private items.
fn run(cmd: &mut Command) -> Result<()> {
    let cmd_str = format!("{:?}", cmd);
    println!("   $ {}", cmd_str);
    let status = cmd.status().map_err(BuildError::Io)?;
    if !status.success() {
        return Err(BuildError::CommandFailed {
            cmd: cmd_str,
            code: status.code(),
        });
    }
    Ok(())
}

/// The standard `include/` + `lib/`(`64`) layout that CMake, Meson, and
/// Make (when it cooperates) all install into. Shared by all three
/// builders so the layout convention lives in one place.
fn collect_from_prefix(prefix: &Path) -> BuildOutput {
    let mut lib_dirs = Vec::new();
    for candidate in ["lib", "lib64"] {
        let p = prefix.join(candidate);
        if p.exists() {
            lib_dirs.push(p);
        }
    }
    BuildOutput {
        include_dirs: vec![prefix.join("include")],
        lib_dirs,
        libs: Vec::new(),
    }
}
