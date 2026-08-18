// SPDX-License-Identifier: MIT OR Apache-2.0
// Copyright (c) 2026 Max-Mend
// This file is part of smidr: https://github.com/Max-Mend/smidr

//! Command-line argument definitions.
//!
//! This module only parses arguments — it has no knowledge of how
//! commands are executed. Dispatching a parsed [`Commands`] to the
//! right function is [`crate::main`]'s job, kept separate so the CLI
//! layer can be tested (or replaced) independently of the underlying
//! logic.

use crate::config::ProjectType;
use clap::{Parser, Subcommand};

/// Top-level CLI definition for the `smidr` binary.
#[derive(Parser)]
#[command(name = "Smidr", version = "0.1.0", long_about = None)]
pub struct Cli {
    #[command(subcommand)]
    pub command: Commands,
}

/// The available `smidr` subcommands.
#[derive(Subcommand)]
pub enum Commands {
    /// Scaffold a new project.
    New {
        name: String,
        /// Explicit project kind: `static` or `dynamic`. Cannot be
        /// combined with `--lib`. Defaults to a binary if neither this
        /// nor `--lib` is given.
        #[arg(long, value_enum, conflicts_with = "lib")]
        r#type: Option<ProjectType>,
        /// Shorthand for `--type static` — scaffold a library instead of
        /// a binary (the common case; use `--type dynamic` if you
        /// specifically need a shared object).
        #[arg(long)]
        lib: bool,
    },
    /// Compile the current project.
    Build,
    /// Compile and run the current project.
    Run,
    /// Remove the `target/` build directory.
    Clean,
    /// Clean, then compile the current project from scratch.
    Rebuild,

    // TODO: Add:
    //
    // Add, // add new dependency
    // Remove, // remove dependency
    // Test, // run tests
    // Profile, // profile build
    // Analyze, // analyze code
    // Format, // format code
    // Lint, // lint code
    // Version, // print version
    // Update, // update smidr
}

/// Parse `std::env::args()` into a [`Cli`], exiting the process with a
/// usage message on invalid input (standard `clap` behavior).
pub fn parse_args() -> Cli {
    Cli::parse()
}