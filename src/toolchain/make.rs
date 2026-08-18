// SPDX-License-Identifier: MIT OR Apache-2.0
// Copyright (c) 2026 Max-Mend
// This file is part of smidr: https://github.com/Max-Mend/smidr

//! [`DepBuilder`] implementation for plain Makefile-based dependencies.
//!
//! Unlike CMake and Meson, a bare Makefile has no standard convention for
//! an install prefix - this builder does its best (Autotools' `configure
//! --prefix`, then `make install PREFIX=`), but there's no guarantee a
//! given Makefile honors either. When it doesn't, the failure message
//! points at `build_system = "custom"` as the reliable fallback.

use super::{collect_from_prefix, run, BuildOutput, DepBuilder};
use crate::error::{BuildError, Result};
use std::path::Path;
use std::process::Command;

/// Builds a dependency via a plain Makefile, optionally preceded by an
/// Autotools `./configure` step.
pub struct MakeBuilder;

impl DepBuilder for MakeBuilder {
    /// Detects a bare Makefile by the presence of `Makefile` or `makefile`.
    fn detect(src_dir: &Path) -> bool {
        src_dir.join("Makefile").exists() || src_dir.join("makefile").exists()
    }

    fn name(&self) -> &'static str {
        "make"
    }

    /// If a `configure` script is present, runs it with `--prefix` first
    /// (the Autotools convention) - this is more reliable than guessing
    /// at Makefile variables. Then runs `make`, then attempts
    /// `make install PREFIX=<prefix>` (the most common GNU convention for
    /// hand-written Makefiles).
    ///
    /// # Errors
    /// Returns [`BuildError::Dependency`] if the install step fails,
    /// since that most likely means this Makefile doesn't support
    /// `PREFIX=` at all.
    fn build(&self, src_dir: &Path, prefix: &Path) -> Result<BuildOutput> {
        // Autotools convention: if `configure` exists, it determines the
        // prefix for the generated Makefile - more reliable than guessing.
        let configure = src_dir.join("configure");
        if configure.exists() {
            run(Command::new("./configure")
                .arg(format!("--prefix={}", prefix.display()))
                .current_dir(src_dir))?;
        }

        run(Command::new("make").current_dir(src_dir))?;

        // Most common GNU convention for a bare Makefile without configure.
        // Unlike CMake/Meson, there's no guarantee PREFIX= is supported at
        // all by this particular Makefile.
        let install_status = Command::new("make")
            .arg(format!("PREFIX={}", prefix.display()))
            .arg("install")
            .current_dir(src_dir)
            .status()
            .map_err(BuildError::Io)?;

        if !install_status.success() {
            return Err(BuildError::Dependency {
                name: src_dir.display().to_string(),
                reason: "`make install PREFIX=...` did not work - this Makefile \
                         may not support PREFIX=. Set build_system = \"custom\" \
                         with explicit build_commands in Smidr.toml"
                    .to_string(),
            });
        }

        Ok(collect_from_prefix(prefix))
    }
}
