// SPDX-License-Identifier: MIT OR Apache-2.0
// Copyright (c) 2026 Max-Mend
// This file is part of smidr: https://github.com/Max-Mend/smidr

//! [`DepBuilder`] implementation for CMake-based dependencies.

use super::{collect_from_prefix, run, BuildOutput, DepBuilder};
use crate::error::Result;
use std::path::Path;
use std::process::Command;

/// Builds a dependency via `cmake` (configure, build, install).
pub struct CMakeBuilder;

impl DepBuilder for CMakeBuilder {
    /// Detects CMake by the presence of `CMakeLists.txt`.
    fn detect(src_dir: &Path) -> bool {
        src_dir.join("CMakeLists.txt").exists()
    }

    fn name(&self) -> &'static str {
        "cmake"
    }

    /// Runs the standard three-step CMake flow: configure into `build/`
    /// with `CMAKE_INSTALL_PREFIX` set to `prefix`, build, then install.
    fn build(&self, src_dir: &Path, prefix: &Path) -> Result<BuildOutput> {
        // 1. Configure: generate build/ with the desired install prefix.
        run(Command::new("cmake")
            .args(["-S", ".", "-B", "build"])
            .arg(format!("-DCMAKE_INSTALL_PREFIX={}", prefix.display()))
            .arg("-DCMAKE_BUILD_TYPE=Release")
            .arg("-DCMAKE_POLICY_VERSION_MINIMUM=3.5")
            .current_dir(src_dir))?;

        // 2. Build.
        run(Command::new("cmake")
            .args(["--build", "build", "--parallel"])
            .current_dir(src_dir))?;

        // 3. Install into prefix (this is where include/lib come from).
        run(Command::new("cmake")
            .args(["--install", "build"])
            .current_dir(src_dir))?;

        Ok(collect_from_prefix(prefix))
    }
}
