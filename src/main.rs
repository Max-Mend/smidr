// SPDX-License-Identifier: MIT OR Apache-2.0
// Copyright (c) 2026 Max-Mend
// This file is part of smidr: https://github.com/Max-Mend/smidr

//! `smidr` entry point.
//!
//! Kept intentionally thin: parse CLI arguments, dispatch to the
//! relevant module, and report any [`error::BuildError`] to the user.
//! No business logic lives here - see [`project`] for project
//! creation/loading and [`builder`] for compiling and running.

mod builder;
mod cli;
mod compile_db;
mod config;
mod error;
mod project;
mod resolver;
mod toolchain;

use cli::Commands;

fn main() {
    if let Err(e) = run() {
        eprintln!("Error: {}", e);
        std::process::exit(1);
    }
}

/// Parse arguments and dispatch to the requested command. Separated from
/// `main` so the whole command flow returns a `Result` and can use `?`
/// throughout, rather than matching on errors at every call site.
fn run() -> error::Result<()> {
    let cli = cli::parse_args();

    match &cli.command {
        Commands::New { name } => project::Project::init(name),
        Commands::Build => {
            let project = project::Project::load(&std::env::current_dir()?)?;
            builder::build_project(&project)
        }
        Commands::Run => {
            let project = project::Project::load(&std::env::current_dir()?)?;
            builder::run_project(&project)
        }
    }
}
