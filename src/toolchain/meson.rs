// SPDX-License-Identifier: MIT OR Apache-2.0
// Copyright (c) 2026 Max-Mend
// This file is part of smidr: https://github.com/Max-Mend/smidr

//! [`DepBuilder`] implementation for Meson-based dependencies.

use super::{collect_from_prefix, run, BuildOutput, DepBuilder};
use crate::error::Result;
use std::path::Path;
use std::process::Command;

/// Builds a dependency via `meson` (setup, compile, install).
pub struct MesonBuilder;

impl DepBuilder for MesonBuilder {
    /// Detects Meson by the presence of `meson.build`.
    fn detect(src_dir: &Path) -> bool {
        src_dir.join("meson.build").exists()
    }

    fn name(&self) -> &'static str {
        "meson"
    }

    /// Runs the standard three-step Meson flow: setup a release build
    /// into `build/` with `--prefix` set to `prefix`, compile, install.
    fn build(&self, src_dir: &Path, prefix: &Path, verbose: bool) -> Result<BuildOutput> {
        let label = src_dir.file_name().and_then(|s| s.to_str()).unwrap_or("dependency");
        // 1. Setup the build directory with the desired prefix.
        run(Command::new("meson")
                .args(["setup", "build"])
                .arg(format!("--prefix={}", prefix.display()))
                .arg("--buildtype=release")
                .current_dir(src_dir),
            verbose, "Configuring", label)?;
        // 2. Compile.
        run(Command::new("meson").args(["compile", "-C", "build"]).current_dir(src_dir),
            verbose, "Building", label)?;
        // 3. Install into prefix.
        run(Command::new("meson").args(["install", "-C", "build"]).current_dir(src_dir),
            verbose, "Installing", label)?;

        Ok(collect_from_prefix(prefix))
    }
}
