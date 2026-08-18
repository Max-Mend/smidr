// SPDX-License-Identifier: MIT OR Apache-2.0
// Copyright (c) 2026 Max-Mend
// This file is part of smidr: https://github.com/Max-Mend/smidr

//! [`DepBuilder`] implementation for user-defined shell build steps.
//!
//! Used when none of the built-in build systems fit, or when the user
//! wants full control. **Security note:** `build()` executes the
//! `build_commands` from `Smidr.toml` verbatim via `sh -c` — running a
//! `Smidr.toml` from an untrusted source is equivalent to running its
//! shell commands directly.

use super::{collect_from_prefix, run, BuildOutput, DepBuilder};
use crate::config::DependencySpec;
use crate::error::{BuildError, Result};
use std::path::Path;
use std::process::Command;

/// Runs arbitrary shell commands from `Smidr.toml`
/// (`build_commands`) when no built-in build system applies.
pub struct CustomBuilder {
    commands: Vec<String>,
    libs: Vec<String>,
    extra_includes: Vec<String>,
}

impl CustomBuilder {
    /// Build a [`CustomBuilder`] from a dependency's config, cloning out
    /// only the fields this builder needs.
    pub fn new(spec: &DependencySpec) -> Self {
        Self {
            commands: spec.build_commands.clone(),
            libs: spec.libs.clone(),
            extra_includes: spec.extra_includes.clone(),
        }
    }
}

impl DepBuilder for CustomBuilder {
    /// Always `false` - `custom` is never auto-selected by
    /// `BuildSystemKind::Auto`; it only runs when explicitly requested in
    /// `Smidr.toml`.
    fn detect(_src_dir: &Path) -> bool {
        false
    }

    fn name(&self) -> &'static str {
        "custom"
    }

    /// Runs each command in `build_commands` via `sh -c`, in order, in
    /// `src_dir`. `$SMIDR_PREFIX` in a command is substituted with the
    /// dependency's install prefix before execution.
    ///
    /// Since there's no build system here to introspect, the resulting
    /// [`BuildOutput`] is built from the standard prefix layout plus
    /// whatever the user explicitly listed in `libs` and
    /// `extra_includes` - nothing is guessed.
    ///
    /// # Errors
    /// Returns [`BuildError::Dependency`] if `build_commands` is empty.
    fn build(&self, src_dir: &Path, prefix: &Path) -> Result<BuildOutput> {
        if self.commands.is_empty() {
            return Err(BuildError::Dependency {
                name: src_dir.display().to_string(),
                reason: "build_system = \"custom\", but build_commands is empty \
                         in Smidr.toml"
                    .to_string(),
            });
        }

        for raw_cmd in &self.commands {
            // Let the user reference the install prefix in their commands,
            // e.g. "make install PREFIX=$SMIDR_PREFIX"
            let expanded = raw_cmd.replace("$SMIDR_PREFIX", &prefix.display().to_string());
            run(Command::new("sh")
                .arg("-c")
                .arg(&expanded)
                .current_dir(src_dir))?;
        }

        let mut output = collect_from_prefix(prefix);
        output.libs = self.libs.clone();
        for extra in &self.extra_includes {
            output.include_dirs.push(prefix.join(extra));
        }
        Ok(output)
    }
}
