// SPDX-License-Identifier: MIT OR Apache-2.0
// Copyright (c) 2026 Max-Mend
// This file is part of smidr: https://github.com/Max-Mend/smidr

//! Generates `compile_commands.json`, the de facto standard format
//! consumed by `clangd` and other C/C++ language servers for accurate
//! code completion and diagnostics.
//!
//! This module only knows how to serialize a list of entries to disk —
//! building that list from the actual compile commands run during a
//! build is [`crate::builder`]'s job (not yet wired up; see the crate's
//! roadmap).

use crate::error::Result;
use serde::Serialize;
use std::path::Path;

/// One entry in `compile_commands.json`, matching the format's
/// conventional field names.
#[derive(Serialize, Debug)]
pub struct CompileCommand {
    /// The working directory the command was run from.
    pub directory: String,
    /// The source file this command compiles.
    pub file: String,
    /// The full compiler invocation, as a single string.
    pub command: String,
    /// The output file the command produces.
    pub output: String,
}

/// Serialize `entries` as pretty-printed JSON and write them to
/// `out_path` (conventionally `compile_commands.json` in the project
/// root).
pub fn write(entries: &[CompileCommand], out_path: &Path) -> Result<()> {
    let json = serde_json::to_string_pretty(entries)?;
    std::fs::write(out_path, json)?;

    Ok(())
}
