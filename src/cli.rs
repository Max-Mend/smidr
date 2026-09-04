// SPDX-License-Identifier: MIT OR Apache-2.0
// Copyright (c) 2026 Max-Mend
// This file is part of smidr: https://github.com/Max-Mend/smidr

//! Command-line argument definitions.
//!
//! This module only parses arguments - it has no knowledge of how
//! commands are executed. Dispatching a parsed [`Commands`] to the
//! right function is [`crate::main`]'s job, kept separate so the CLI
//! layer can be tested (or replaced) independently of the underlying
//! logic.

use crate::config::{LanguageStandard, ProjectType};
use clap::{Parser, Subcommand};

/// Top-level CLI definition for the `smidr` binary.
#[derive(Parser)]
#[command(name = "Smidr", version = "0.8.0", long_about = None)]
pub struct Cli {
    #[command(subcommand)]
    pub command: Commands,
}

/// The available `smidr` subcommands.
#[derive(Subcommand)]
pub enum Commands {
    /// Scaffold a new project.
    #[command(alias = "n")]
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
        /// Specify the C or C++ language standard to use (e.g., c17, c23, cpp20).
        /// Defaults to c17.
        #[arg(long)]
        std: Option<LanguageStandard>,
    },
    /// Compile the current project.
    #[command(alias = "b")]
    Build {
        #[arg(long)]
        release: bool,

        #[arg(long)]
        verbose: bool,

        #[arg(long)]
        dry_run: bool,
    },
    /// Compile and run the current project.
    #[command(alias = "r")]
    Run {
        #[arg(long)]
        release: bool,

        #[arg(long)]
        verbose: bool,

        #[arg(long)]
        dry_run: bool,
    },
    /// Remove the `target/` build directory.
    #[command(alias = "cl")]
    Clean,
    /// Clean, then compile the current project from scratch.
    #[command(alias = "rb")]
    Rebuild {
        #[arg(long)]
        release: bool,

        #[arg(long)]
        verbose: bool,
        
        #[arg(long)]
        dry_run: bool,
    },
    /// Format project source and header files with clang-format.
    #[command(alias = "fmt")]
    Format,
    /// Add a dependency to the project.
    #[command(alias = "a")]
    Add { name: String },
    /// Remove a dependency from the project.
    #[command(alias = "rm")]
    Remove { name: String },
    /// Check source files for syntax errors without compiling.
    #[command(alias = "l")]
    Lint,
    /// Update smidr to the latest version.
    #[command(alias = "up")]
    Update,
}

/// Parse `std::env::args()` into a [`Cli`], exiting the process with a
/// usage message on invalid input (standard `clap` behavior).
pub fn parse_args() -> Cli {
    Cli::parse()
}