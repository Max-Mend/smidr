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

fn run() -> error::Result<()> {
    let cli = cli::parse_args();

    match &cli.command {
        Commands::New { name, r#type, lib } => {
            let project_type = match (r#type, lib) {
                (Some(t), _) => t.clone(),
                (None, true) => config::ProjectType::StaticLibrary,
                (None, false) => config::ProjectType::Binary,
            };
            project::Project::init(name, project_type)
        }
        Commands::Build => {
            let project = project::Project::load(&std::env::current_dir()?)?;
            builder::build_project(&project)
        }
        Commands::Run => {
            let project = project::Project::load(&std::env::current_dir()?)?;
            builder::run_project(&project)
        }
        Commands::Clean => {
            let target_dir = std::path::Path::new("target");
            if target_dir.exists() {
                if let Err(e) = std::fs::remove_dir_all(target_dir) {
                    eprintln!("Error: Failed to remove target directory: {}", e);
                    std::process::exit(1);
                }
                println!("Cleaned target directory.");
            } else {
                println!("Nothing to clean.");
            }
            Ok(())
        }
    }
}