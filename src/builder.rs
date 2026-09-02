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

use crate::compile_db::CompileCommand;
use crate::diagnostics::{Diagnostic, print_diagnostic};
use crate::error::Result;
use crate::project::Project;
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
    dep_libs: Vec<String>,
    dep_lib_dirs: Vec<PathBuf>,
}

/// Compile every `.c` file in `project` and link them into a binary at
/// `target/bin/<project-name>`.
///
/// Steps: resolve a compiler ([`compiler_binary`]), compile each source
/// file to `target/<name>.o`, then link all object files together. A
/// `compile_commands.json` (recording the exact command used for each
/// file) is written to the project root once all files have compiled
/// successfully.
///
/// # Errors
/// Returns [`crate::error::BuildError::CompilerNotFound`] if no usable
/// compiler is found, [`crate::error::BuildError::Compile`] if a source
/// file fails to compile, or [`crate::error::BuildError::Link`] if the
/// final link step fails.
pub fn build_project(project: &Project, release: bool) -> Result<()> {
    let project_section = project.config.project.as_ref().ok_or_else(|| {
        crate::error::BuildError::Dependency {
            name: project.root.display().to_string(),
            reason: "cannot build: Smidr.toml has no [project] section (this is a workspace root)".to_string(),
        }
    })?;
    let build_section = project.config.build.as_ref().ok_or_else(|| {
        crate::error::BuildError::Dependency {
            name: project.root.display().to_string(),
            reason: "cannot build: Smidr.toml has no [build] section".to_string(),
        }
    })?;

    let sources = project.source_files()?;
    project.header_files()?;

    let profile_dir = if release { "release" } else { "debug" };
    let build_dir = project.build_dir.join(profile_dir);
    std::fs::create_dir_all(&build_dir)?;

    let compiler = compiler_binary(&build_section.compiler)?;
    println!("Using compiler: {}", compiler);

    let profile = if release {
        project.config.get_release_profile()
    } else {
        project.config.get_debug_profile()
    };

    let mut opts = CompileOptions {
        compiler: compiler.to_string(),
        includes: vec![project.root.join("include")],
        cflags: build_section.cflags.clone(),
        dep_libs: Vec::new(),
        dep_lib_dirs: Vec::new(),
    };

    for (_name, output) in &project.resolved_deps {
        opts.includes.extend(output.include_dirs.clone());
        opts.dep_lib_dirs.extend(output.lib_dirs.clone());
        opts.dep_libs.extend(output.libs.clone());
    }

    let mut object_files: Vec<PathBuf> = Vec::new();
    let mut compile_commands: Vec<CompileCommand> = Vec::new();

    for src in sources {
        let file_stem = src.file_stem().unwrap().to_str().unwrap();
        let obj_path = build_dir.join(format!("{}.o", file_stem));

        let mut cmd = std::process::Command::new(&opts.compiler);
        cmd.arg("-c").arg(&src).arg("-o").arg(&obj_path);
        cmd.args(
            &opts
                .includes
                .iter()
                .map(|p| format!("-I{}", p.display()))
                .collect::<Vec<_>>(),
        );
        cmd.arg(match profile.opt_level {
            crate::config::OptLevel::None => "-O0",
            crate::config::OptLevel::Speed => "-O2",
            crate::config::OptLevel::Size => "-Os",
            crate::config::OptLevel::Max => "-O3",
        });
        if profile.debug_symbols {
            cmd.arg("-g");
        }
        cmd.arg(format!("-std={}", project_section.c_standard));

        // Recording the actual command used for this specific file -
        // doing it before .output(), while cmd is still available for formatting,
        // and after all arguments have been added.
        let command_str = format!("{:?}", cmd);

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
            let error_detail = if printed_pretty {
                "See error details above...".to_string()
            } else {
                stderr.to_string()
            };

            return Err(crate::error::BuildError::Compile(
                src.display().to_string(),
                error_detail,
            ));
        }

        compile_commands.push(CompileCommand {
            directory: project.root.display().to_string(),
            file: src.display().to_string(),
            command: command_str,
            output: obj_path.display().to_string(),
        });

        object_files.push(obj_path);
    }

    crate::compile_db::write(
        &compile_commands,
        &project.root.join("compile_commands.json"),
    )?;

    let output_name = project_section.output_name();

    std::fs::create_dir_all(build_dir.join("bin"))?;

    // Linking based on project type
    match project_section.project_type {
        // Binary
        crate::config::ProjectType::Binary => {
            let binary_path = build_dir.join("bin").join(output_name);

            let mut link_cmd = std::process::Command::new(&opts.compiler);
            link_cmd.args(&object_files);
            link_cmd.arg("-o").arg(&binary_path);
            link_cmd.args(opts.dep_lib_dirs.iter().map(|p| format!("-L{}", p.display())));
            link_cmd.args(opts.dep_libs.iter().map(|l| format!("-l{}", l)));
            link_cmd.args(build_section.libs.iter().map(|l| format!("-l{}", l)));
            link_cmd.args(&build_section.linker_flags);
            if profile.lto { link_cmd.arg("-flto"); }
            if profile.strip { link_cmd.arg("-s"); }

            let output = link_cmd.output()?;
            if !output.status.success() {
                return Err(crate::error::BuildError::Link(
                    String::from_utf8_lossy(&output.stderr).to_string(),
                ));
            }
        }

        // Static library
        crate::config::ProjectType::StaticLibrary => {
            let lib_path = build_dir.join("bin").join(format!("lib{}.a", output_name));

            let mut ar_cmd = std::process::Command::new("ar");
            ar_cmd.arg("rcs").arg(&lib_path).args(&object_files);

            let output = ar_cmd.output()?;
            if !output.status.success() {
                return Err(crate::error::BuildError::Link(
                    String::from_utf8_lossy(&output.stderr).to_string(),
                ));
            }
        }

        // Shared library
        crate::config::ProjectType::SharedLibrary => {
            let lib_path = build_dir.join("bin").join(format!("lib{}.so", output_name));

            let mut link_cmd = std::process::Command::new(&opts.compiler);
            link_cmd.arg("-shared").args(&object_files);
            link_cmd.arg("-o").arg(&lib_path);
            link_cmd.args(opts.dep_lib_dirs.iter().map(|p| format!("-L{}", p.display())));
            link_cmd.args(opts.dep_libs.iter().map(|l| format!("-l{}", l)));
            link_cmd.args(build_section.libs.iter().map(|l| format!("-l{}", l)));
            link_cmd.args(&build_section.linker_flags);

            let output = link_cmd.output()?;
            if !output.status.success() {
                return Err(crate::error::BuildError::Link(
                    String::from_utf8_lossy(&output.stderr).to_string(),
                ));
            }
        }
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
pub fn run_project(project: &Project, release: bool) -> Result<()> {
    build_project(project, release)?;
    let project_section = project.config.project.as_ref().ok_or_else(|| {
        crate::error::BuildError::Dependency {
            name: project.root.display().to_string(),
            reason: "cannot run: Smidr.toml has no [project] section".to_string(),
        }
    })?;
    let profile_dir = if release { "release" } else { "debug" };
    let binary_path = project.build_dir.join(profile_dir).join("bin").join(
        project_section
            .output_name
            .as_ref()
            .unwrap_or(&project_section.name),
    );

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

pub fn rebuild_project(project: &Project, release: bool) -> Result<()> {
    clean_project(project)?;
    build_project(project, release)
}

/// Format project source and header files with clang-format.
///
/// # Errors
/// Returns [crate::error::BuildError::CompilerNotFound] if
/// clang-format isn't on PATH. Returns
/// [crate::error::BuildError::CommandFailed] if clang-format exits
/// with a non-zero status.
pub fn fmt_project(project: &Project) -> Result<()> {
    if !command_exists("clang-format") {
        return Err(crate::error::BuildError::ToolNotFound {
            tool: "clang-format".to_string(),
            hint: "Install it via your package manager (e.g. `apt install clang-format`)."
                .to_string(),
        });
    }

    let files = project.formattable_files()?;
    if files.is_empty() {
        println!("Nothing to format.");
        return Ok(());
    }

    let mut cmd = std::process::Command::new("clang-format");
    cmd.arg("-i");
    cmd.args(&files);

    let status = cmd.status()?;
    if !status.success() {
        return Err(crate::error::BuildError::CommandFailed {
            cmd: "clang-format".to_string(),
            code: status.code(),
        });
    }

    println!("Formatted {} file(s).", files.len());
    Ok(())
}

/// Resolve a [`crate::config::CompilerKind`] into an actual compiler
/// binary name, verifying it's runnable rather than trusting the config
/// blindly.
///
/// An explicit choice (`Gcc`/`Tcc`/`Clang`) is checked against the system
/// before use - better to fail clearly here than have the compiler
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
            if command_exists("gcc") {
                Ok("gcc")
            } else {
                Err(crate::error::BuildError::CompilerNotFound(
                    "gcc".to_string(),
                ))
            }
        }
        CompilerKind::Tcc => {
            if command_exists("tcc") {
                Ok("tcc")
            } else {
                Err(crate::error::BuildError::CompilerNotFound(
                    "tcc".to_string(),
                ))
            }
        }
        CompilerKind::Clang => {
            if command_exists("clang") {
                Ok("clang")
            } else {
                Err(crate::error::BuildError::CompilerNotFound(
                    "clang".to_string(),
                ))
            }
        }
        CompilerKind::Auto => {
            for candidate in ["clang", "tcc", "cc", "gcc"] {
                if command_exists(candidate) {
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
fn command_exists(name: &str) -> bool {
    std::process::Command::new(name)
        .arg("--version")
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .map(|status| status.success())
        .unwrap_or(false)
}
