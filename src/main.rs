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
mod diagnostics;
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
        Commands::New { name, r#type, lib, std } => {
            let project_type = match (r#type, lib) {
                (Some(t), _) => t.clone(),
                (None, true) => config::ProjectType::StaticLibrary,
                (None, false) => config::ProjectType::Binary,
            };
            project::Project::init(name, project_type, std.clone().map(Into::into))
        }
        Commands::Build { release, verbose, dry_run } => build_current(*release, *verbose, *dry_run),
        Commands::Run { release, verbose, dry_run } => run_current(*release, *verbose, *dry_run),
        Commands::Clean => {
            let project = project::Project::load(&std::env::current_dir()?)?;
            builder::clean_project(&project)
        }
        Commands::Rebuild { release, verbose, dry_run } => {
            let mut project = project::Project::load(&std::env::current_dir()?)?;
            project.resolve_dependencies(*release)?;
            builder::rebuild_project(&project, *release, *verbose, *dry_run)
        }
        Commands::Format => {
            let project = project::Project::load(&std::env::current_dir()?)?;
            builder::fmt_project(&project)
        }
        Commands::Add { name } => {
            let mut project = project::Project::load(&std::env::current_dir()?)?;
            project.add_dependency(name)
        }
        Commands::Remove { name } => {
            let mut project = project::Project::load(&std::env::current_dir()?)?;
            project.remove_dependency(name)
        }
        Commands::Lint => {
            let project = project::Project::load(&std::env::current_dir()?)?;
            builder::lint_project(&project)
        }
        Commands::Update => builder::update_project(),
    }
}

fn build_current(release: bool, verbose: bool, dry_run: bool) -> error::Result<()> {
    let cwd = std::env::current_dir()?;
    let raw_config = config::ManifestConfig::load(&cwd)?;

    // If there are workspace members - build them first
    if let Some(workspace) = &raw_config.workspace {
        build_workspace(&cwd, workspace, release, verbose, dry_run)?;
    }

    // If there is a [project] section in the root - build it too
    if raw_config.project.is_some() {
        let mut project = project::Project::load(&cwd)?;
        project.resolve_dependencies(release)?;
        builder::build_project(&project, release, verbose, dry_run)?;
    }

    Ok(())
}

fn run_current(release: bool, verbose: bool, dry_run: bool) -> error::Result<()> {
    let cwd = std::env::current_dir()?;
    let raw_config = config::ManifestConfig::load(&cwd)?;

    if raw_config.project.is_none() {
        return Err(error::BuildError::Dependency {
            name: cwd.display().to_string(),
            reason: "cannot run: this Smidr.toml has no [project] section (workspace root only). \
                     Run from inside a specific member directory instead.".to_string(),
        });
    }

    if let Some(workspace) = &raw_config.workspace {
        build_workspace(&cwd, workspace, release, verbose, dry_run)?;
    }

    let mut project = project::Project::load(&cwd)?;
    project.resolve_dependencies(release)?;
    builder::run_project(&project, release, verbose, dry_run)
}

fn build_workspace(
    root: &std::path::Path,
    workspace: &config::WorkspacesConfig,
    release: bool,
    verbose: bool,
    dry_run: bool,
) -> error::Result<()> {
    for member in &workspace.members {
        let member_path = root.join(member);
        println!("Building workspace member: {}", member);

        let mut member_project = project::Project::load(&member_path)?;
        member_project.resolve_dependencies(release)?;
        builder::build_project(&member_project, release, verbose, dry_run)?;
    }
    Ok(())
}