// SPDX-License-Identifier: MIT OR Apache-2.0
// Copyright (c) 2026 Max-Mend
// This file is part of smidr: https://github.com/Max-Mend/smidr

//! Compiles and links a [`Project`]'s `.c` sources into a binary.
//!
//! This is the one module that ties everything else together: it reads
//! [`crate::config`] settings off a [`Project`], resolves a concrete
//! compiler binary, invokes it once per source file, and links the
//! results. Dependency include paths (`project.resolved_deps`) are
//! folded in, but the step that actually populates `resolved_deps` -
//! calling [`crate::resolver`] and [`crate::toolchain`] - isn't wired up
//! here yet (see the crate's roadmap).

use crate::error::Result;
use crate::project::Project;
use crate::diagnostics::{Diagnostic, print_diagnostic};
use std::path::PathBuf;

/// A single compiled `.c` file, paired with the exact command used to
/// produce it. Currently unused - intended for feeding
/// [`crate::compile_db`] once that's wired into `build_project`.
pub struct CompiledObject {
    obj_path: PathBuf,
    compile_command: String,
}

/// Everything needed to invoke the compiler: which binary, which include
/// paths, and which extra flags.
pub struct CompileOptions {
    compiler: String,
    includes: Vec<PathBuf>,
    cflags: Vec<String>,
}

/// Compile every `.c` file in `project` and link them into a binary at
/// `target/bin/<project-name>`.
///
/// Steps: resolve a compiler ([`compiler_binary`]), compile each source
/// file to `target/<name>.o`, then link all object files together.
///
/// # Errors
/// Returns [`crate::error::BuildError::CompilerNotFound`] if no usable
/// compiler is found, [`crate::error::BuildError::Compile`] if a source
/// file fails to compile, or [`crate::error::BuildError::Link`] if the
/// final link step fails.
pub fn build_project(project: &Project) -> Result<()> {
    let sources = project.source_files()?;
    std::fs::create_dir_all(&project.build_dir)?;

    let compiler = compiler_binary(&project.config.build.compiler)?;
    println!("Using compiler: {}", compiler);

    let mut opts = CompileOptions {
        compiler: compiler.to_string(),
        includes: vec![project.root.join("include")],
        cflags: project.config.build.cflags.clone(),
    };

    for (name, output) in &project.resolved_deps {
        let prefix = project.dep_prefix(name);
        opts.includes.extend(output.include_dirs.clone());
    }

    let mut object_files: Vec<PathBuf> = Vec::new();

    for src in sources {
        let file_stem = src.file_stem().unwrap().to_str().unwrap();
        let obj_path = project.build_dir.join(format!("{}.o", file_stem));

        let mut cmd = std::process::Command::new(&opts.compiler);
        cmd.arg("-c").arg(&src).arg("-o").arg(&obj_path);
        cmd.args(
            &opts
                .includes
                .iter()
                .map(|p| format!("-I{}", p.display()))
                .collect::<Vec<_>>(),
        );
        cmd.args(&opts.cflags);

        cmd.arg(format!("-std={}", project.config.project.c_standard));

        let output = cmd.output()?;
        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            let mut printed_pretty = false;

            for line in stderr.lines() {
                if let Some(diag) = Diagnostic::parse_line(line) {
                    print_diagnostic(&diag);
                    printed_pretty = true;
                    break;
                }
            }
<<<<<<< HEAD
            
=======

>>>>>>> dev
            let error_detail = if printed_pretty {
                "See error details above..".to_string()
            } else {
                stderr.to_string()
            };

            return Err(crate::error::BuildError::Compile(
                src.display().to_string(),
                error_detail,
            ));
        }
        object_files.push(obj_path);
    }

    let binary_path = project
        .build_dir
        .join("bin")
        .join(&project.config.project.name);
    std::fs::create_dir_all(project.build_dir.join("bin"))?;

    let mut link_cmd = std::process::Command::new(&opts.compiler);
    link_cmd.args(&object_files);
    link_cmd.arg("-o").arg(&binary_path);

    let output = link_cmd.output()?;
    if !output.status.success() {
        return Err(crate::error::BuildError::Link(
            String::from_utf8_lossy(&output.stderr).to_string(),
        ));
    }

    Ok(())
}

/// Build `project` (via [`build_project`]) and then execute the
/// resulting binary, forwarding its exit status.
///
/// # Errors
/// Propagates any error from [`build_project`]. Returns
/// [`crate::error::BuildError::CommandFailed`] if the binary itself
/// exits with a non-zero status.
pub fn run_project(project: &Project) -> Result<()> {
    build_project(project)?;
    let binary_path = project
        .build_dir
        .join("bin")
        .join(&project.config.project.name);

    println!("Running: {}", binary_path.display());
    let status = std::process::Command::new(&binary_path).status()?;

    if !status.success() {
        return Err(crate::error::BuildError::CommandFailed {
            cmd: binary_path.display().to_string(),
            code: status.code(),
        });
    }

    Ok(())
}

/// Remove build artifacts from a project.
///
/// # Errors
/// Propagates any error from [`std::fs::remove_dir_all`].
pub fn clean_project(project: &Project) -> Result<()> {
    if project.build_dir.exists() {
        std::fs::remove_dir_all(&project.build_dir)?;
        println!("Cleaned: {}", project.build_dir.display());
    } else {
        println!("Nothing to clean.");
    }
    Ok(())
}

pub fn rebuild_project(project: &Project) -> Result<()> {
    clean_project(project)?;
    build_project(project)
}

/// Resolve a [`crate::config::CompilerKind`] into an actual compiler
/// binary name, verifying it's runnable rather than trusting the config
/// blindly.
///
/// An explicit choice (`Gcc`/`Tcc`/`Clang`) is checked against the system
/// before use — better to fail clearly here than have the compiler
/// invocation fail later with a confusing "command not found".
/// `Auto` tries, in priority order: `clang`, `tcc`, the system `cc`,
/// then `gcc` as a last resort.
///
/// # Errors
/// Returns [`crate::error::BuildError::CompilerNotFound`] if the
/// requested compiler (or, for `Auto`, none of the candidates) is found
/// on `PATH`.
fn compiler_binary(kind: &crate::config::CompilerKind) -> Result<&'static str> {
    use crate::config::CompilerKind;

    match kind {
        CompilerKind::Gcc => {
            if compiler_exists("gcc") {
                Ok("gcc")
            } else {
                Err(crate::error::BuildError::CompilerNotFound(
                    "gcc".to_string(),
                ))
            }
        }
        CompilerKind::Tcc => {
            if compiler_exists("tcc") {
                Ok("tcc")
            } else {
                Err(crate::error::BuildError::CompilerNotFound(
                    "tcc".to_string(),
                ))
            }
        }
        CompilerKind::Clang => {
            if compiler_exists("clang") {
                Ok("clang")
            } else {
                Err(crate::error::BuildError::CompilerNotFound(
                    "clang".to_string(),
                ))
            }
        }
        CompilerKind::Auto => {
            for candidate in ["clang", "tcc", "cc", "gcc"] {
                if compiler_exists(candidate) {
                    return Ok(candidate);
                }
            }
            Err(crate::error::BuildError::CompilerNotFound(
                "clang, tcc, cc, gcc".to_string(),
            ))
        }
    }
}

/// Check whether `name` is a runnable compiler on `PATH`, by attempting
/// to run `<name> --version` and discarding its output.
fn compiler_exists(name: &str) -> bool {
    std::process::Command::new(name)
        .arg("--version")
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .map(|status| status.success())
        .unwrap_or(false)
}
