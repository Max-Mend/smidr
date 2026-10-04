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
//! calling [`crate::resolver`] and [`crate::toolchain`] - is not wired up
//! here yet (see the crate's roadmap).

use crate::compile_db::CompileCommand;
use crate::diagnostics::{parse_all, print_all};
use crate::error::Result;
use crate::project::Project;
use std::path::PathBuf;

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
pub fn build_project(project: &Project, release: bool, verbose: bool, dry_run: bool, incremental: bool) -> Result<()> {
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

    let mut sources = project.source_files()?;
    let headers = project.header_files()?;

    let profile_dir = if release { "release" } else { "debug" };
    let build_dir = project.build_dir.join(profile_dir);
    std::fs::create_dir_all(&build_dir)?;

    if project_section.language == crate::config::Language::Cpp {
        if let Some(moc_bin) = find_moc_binary() {
            let moc_generated = run_moc(&headers, &build_dir, &moc_bin, verbose)?;
            sources.extend(moc_generated);
        }

        let qrc = qrc_files(project);
        if !qrc.is_empty() {
            if let Some(rcc_bin) = find_rcc_binary() {
                let rcc_generated = run_rcc(&qrc, &build_dir, &rcc_bin, verbose)?;
                sources.extend(rcc_generated);
            }
        }
    }

    let compiler = compiler_binary(&build_section.compiler, &project_section.language)?;
    println!("Using compiler: {}", compiler);

    let profile = if release {
        project.config.get_release_profile()
    } else {
        project.config.get_debug_profile()
    };

    let mut opts = CompileOptions {
        compiler: compiler.to_string(),
        includes: {
            let mut incs = vec![project.root.join(
                project.config.paths.include.as_deref().unwrap_or("include")
            )];
            incs.extend(project.config.paths.custom.values().map(|p| project.root.join(p)));
            incs
        },
        cflags: build_section.cflags.clone(),
        dep_libs: Vec::new(),
        dep_lib_dirs: Vec::new(),
    };

    for (_name, output) in &project.resolved_deps {
        opts.includes.extend(output.include_dirs.clone());
        opts.dep_lib_dirs.extend(output.lib_dirs.clone());
        opts.dep_libs.extend(output.libs.clone());
    }

    let newest_header_mtime = if incremental {
        project.header_files().ok().and_then(|headers| {
            headers
                .iter()
                .filter_map(|h| h.metadata().and_then(|m| m.modified()).ok())
                .max()
        })
    } else {
        None
    };

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
        cmd.args(&opts.cflags);
        cmd.arg(match profile.opt_level {
            crate::config::OptLevel::None => "-O0",
            crate::config::OptLevel::Speed => "-O2",
            crate::config::OptLevel::Size => "-Os",
            crate::config::OptLevel::Max => "-O3",
        });
        if profile.debug_symbols {
            cmd.arg("-g");
        }
        let std_flag = match project_section.language {
            crate::config::Language::C => format!(
                "-std={}",
                project_section.c_standard.clone().unwrap_or_default()
            ),
            crate::config::Language::Cpp => format!(
                "-std={}",
                project_section.cpp_standard.as_ref()
                    .map(|s| s.to_string())
                    .unwrap_or_else(|| crate::config::CppStandard::default().to_string())
            ),
        };
        cmd.arg(std_flag);

        // Recording the actual command used for this specific file -
        // doing it before .output(), while cmd is still available for formatting,
        // and after all arguments have been added.
        let command_str = format!("{:?}", cmd);

        // Check if the source file is up to date with the object file and the newest header file
        let up_to_date = incremental && !dry_run && {
            let src_mtime = src.metadata().and_then(|m| m.modified()).ok();
            let obj_mtime = obj_path.metadata().and_then(|m| m.modified()).ok();
            match (src_mtime, obj_mtime) {
                (Some(s), Some(o)) => s <= o && newest_header_mtime.map_or(true, |h| h <= o),
                _ => false,
            }
        };

        if up_to_date {
            crate::diagnostics::print_status("Up to date", &src.display().to_string());
            compile_commands.push(CompileCommand {
                directory: project.root.display().to_string(),
                file: src.display().to_string(),
                command: command_str,
                output: obj_path.display().to_string(),
            });
            object_files.push(obj_path);
            continue;
        }

        crate::diagnostics::print_status(
            if dry_run { "Would compile" } else { "Compiling" },
            &src.display().to_string(),
        );
        if verbose {
            println!("      $ {}", command_str);
        }

        if dry_run {
            compile_commands.push(CompileCommand {
                directory: project.root.display().to_string(),
                file: src.display().to_string(),
                command: command_str,
                output: obj_path.display().to_string(),
            });
            object_files.push(obj_path);
            continue;
        }

        let output = cmd.output()?;
        let stderr = String::from_utf8_lossy(&output.stderr);
        let diagnostics = parse_all(&stderr);

        if !diagnostics.is_empty() {
            print_all(&diagnostics);
        }

        if !output.status.success() {
            let error_detail = if diagnostics.is_empty() {
                stderr.to_string()
            } else {
                "See error details above...".to_string()
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
            let binary_name = if cfg!(windows) {
                format!("{}.exe", output_name)
            } else {
                output_name.to_string()
            };
            let binary_path = build_dir.join("bin").join(binary_name);

            let mut link_cmd = std::process::Command::new(&opts.compiler);
            link_cmd.args(&object_files);
            link_cmd.arg("-o").arg(&binary_path);
            link_cmd.args(opts.dep_lib_dirs.iter().map(|p| format!("-L{}", p.display())));
            link_cmd.args(opts.dep_libs.iter().map(|l| format!("-l{}", l)));
            link_cmd.args(build_section.libs.iter().map(|l| format!("-l{}", l)));
            link_cmd.args(&build_section.linker_flags);
            if profile.lto { link_cmd.arg("-flto"); }
            if profile.strip { link_cmd.arg("-s"); }

            crate::diagnostics::print_status(
                if dry_run { "Would link" } else { "Linking" },
                &binary_path.display().to_string(),
            );
            if verbose {
                println!("      $ {:?}", link_cmd);
            }
            if dry_run {
                return Ok(());
            }

            let output = link_cmd.output()?;
            if !output.status.success() {
                return Err(crate::error::BuildError::Link(
                    String::from_utf8_lossy(&output.stderr).to_string(),
                ));
            }
        }

        // Static library
        crate::config::ProjectType::StaticLibrary => {
            let lib_ext = if cfg!(windows) { "lib" } else { "a" };
            let lib_path = build_dir.join("bin").join(format!("lib{}.{}", output_name, lib_ext));

            let mut ar_cmd = std::process::Command::new("ar");
            ar_cmd.arg("rcs").arg(&lib_path).args(&object_files);

            crate::diagnostics::print_status(
                if dry_run { "Would archive" } else { "Archiving" },
                &lib_path.display().to_string(),
            );
            if verbose {
                println!("      $ {:?}", ar_cmd);
            }
            if dry_run {
                return Ok(());
            }

            let output = ar_cmd.output()?;
            if !output.status.success() {
                return Err(crate::error::BuildError::Link(
                    String::from_utf8_lossy(&output.stderr).to_string(),
                ));
            }
        }

        // Shared library
        crate::config::ProjectType::SharedLibrary => {
            let lib_ext = if cfg!(windows) { "dll" } else { "so" };
            let lib_path = build_dir.join("bin").join(format!("lib{}.{}", output_name, lib_ext));

            let mut link_cmd = std::process::Command::new(&opts.compiler);
            link_cmd.arg("-shared").args(&object_files);
            link_cmd.arg("-o").arg(&lib_path);
            link_cmd.args(opts.dep_lib_dirs.iter().map(|p| format!("-L{}", p.display())));
            link_cmd.args(opts.dep_libs.iter().map(|l| format!("-l{}", l)));
            link_cmd.args(build_section.libs.iter().map(|l| format!("-l{}", l)));
            link_cmd.args(&build_section.linker_flags);

            crate::diagnostics::print_status(
                if dry_run { "Would link" } else { "Linking" },
                &lib_path.display().to_string(),
            );
            if verbose {
                println!("      $ {:?}", link_cmd);
            }
            if dry_run {
                return Ok(());
            }

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

// -- moc --

fn find_moc_binary() -> Option<String> {
    let qt_major = ["Qt6Core", "Qt5Core"].iter().find_map(|pkg| {
        std::process::Command::new("pkg-config")
            .arg("--modversion")
            .arg(pkg)
            .output()
            .ok()
            .filter(|o| o.status.success())
            .and_then(|o| {
                String::from_utf8_lossy(&o.stdout)
                    .trim()
                    .split('.')
                    .next()
                    .and_then(|s| s.parse::<u32>().ok())
            })
    })?;

    let mut candidates: Vec<String> = Vec::new();

    // 1. Official Qt mechanism - qtpaths knows where host-tools are located.
    for qtpaths in [format!("qtpaths{qt_major}"), "qtpaths".to_string()] {
        for var in ["QT_HOST_LIBEXECS", "QT_INSTALL_LIBEXECS"] {
            if let Ok(output) = std::process::Command::new(&qtpaths).arg("-query").arg(var).output() {
                if output.status.success() {
                    let dir = String::from_utf8_lossy(&output.stdout).trim().to_string();
                    if !dir.is_empty() {
                        candidates.push(format!("{dir}/moc"));
                    }
                }
            }
        }
    }

    // 2. macOS Homebrew.
    for formula in [format!("qt@{qt_major}"), "qt".to_string()] {
        if let Ok(output) = std::process::Command::new("brew").arg("--prefix").arg(&formula).output() {
            if output.status.success() {
                let prefix = String::from_utf8_lossy(&output.stdout).trim().to_string();
                if !prefix.is_empty() {
                    candidates.push(format!("{prefix}/libexec/moc"));
                    candidates.push(format!("{prefix}/bin/moc"));
                }
            }
        }
    }

    // 3. Typical Linux paths - as a fallback option, if qtpaths is missing.
    for libdir in ["lib", "lib64"] {
        candidates.push(format!("/usr/{libdir}/qt{qt_major}/libexec/moc"));
        candidates.push(format!("/usr/{libdir}/qt{qt_major}/bin/moc"));
        candidates.push(format!("/usr/{libdir}/qt{qt_major}/moc"));
    }

    // 4. Limited disk search - take the prefix from pkg-config and search
    // for the "moc" file no deeper than 3 levels, instead of guessing the
    // specific subdirectory. Saves you when the distribution arranges files
    // somehow non-standardly (as in the cases from the Arch bug tracker).
    if let Ok(output) = std::process::Command::new("pkg-config")
        .arg("--variable=prefix")
        .arg(format!("Qt{qt_major}Core"))
        .output()
    {
        if output.status.success() {
            let prefix = String::from_utf8_lossy(&output.stdout).trim().to_string();
            if !prefix.is_empty() {
                find_file_shallow(std::path::Path::new(&prefix), "moc", 3, &mut candidates);
            }
        }
    }

    candidates.push(format!("moc-qt{qt_major}"));
    candidates.push("moc".to_string());

    for candidate in &candidates {
        if let Ok(output) = std::process::Command::new(candidate).arg("--version").output() {
            if output.status.success() {
                let version_output = String::from_utf8_lossy(&output.stdout);
                if version_output.contains(&format!("{qt_major}.")) {
                    return Some(candidate.clone());
                }
            }
        }
    }

    None
}

/// Search for a file with a given name under a root directory, not deeper than a specified depth.
/// Found files are added to the output vector; 
/// disk access errors are ignored (no permissions, broken symlinks, etc. - not critical here).
fn find_file_shallow(root: &std::path::Path, name: &str, max_depth: u32, out: &mut Vec<String>) {
    if max_depth == 0 {
        return;
    }
    let Ok(entries) = std::fs::read_dir(root) else { return };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            find_file_shallow(&path, name, max_depth - 1, out);
        } else if path.file_name().and_then(|n| n.to_str()) == Some(name) {
            out.push(path.display().to_string());
        }
    }
}

fn needs_moc(header_content: &str) -> bool {
    header_content.contains("Q_OBJECT")
        || header_content.contains("Q_GADGET")
        || header_content.contains("Q_NAMESPACE")
}

fn run_moc(
    headers: &[PathBuf],
    build_dir: &std::path::Path,
    moc_bin: &str,
    verbose: bool,
) -> Result<Vec<PathBuf>> {
    let mut generated = Vec::new();

    for header in headers {
        let content = std::fs::read_to_string(header).unwrap_or_default();
        if !needs_moc(&content) {
            continue;
        }

        let stem = header.file_stem().and_then(|s| s.to_str()).unwrap_or("unknown");
        let moc_out = build_dir.join(format!("moc_{}.cpp", stem));

        let mut cmd = std::process::Command::new(moc_bin);
        cmd.arg(header).arg("-o").arg(&moc_out);

        crate::diagnostics::print_status("Moc", &header.display().to_string());
        if verbose {
            println!("      $ {:?}", cmd);
        }

        let output = cmd.output().map_err(crate::error::BuildError::Io)?;
        if !output.status.success() {
            return Err(crate::error::BuildError::Compile(
                header.display().to_string(),
                String::from_utf8_lossy(&output.stderr).to_string(),
            ));
        }

        generated.push(moc_out);
    }

    Ok(generated)
}

/// -- rcc --

fn find_rcc_binary() -> Option<String> {
    let qt_major = ["Qt6Core", "Qt5Core"].iter().find_map(|pkg| {
        std::process::Command::new("pkg-config")
            .arg("--modversion")
            .arg(pkg)
            .output()
            .ok()
            .filter(|o| o.status.success())
            .and_then(|o| {
                String::from_utf8_lossy(&o.stdout)
                    .trim()
                    .split('.')
                    .next()
                    .and_then(|s| s.parse::<u32>().ok())
            })
    })?;

    let mut candidates: Vec<String> = Vec::new();

    // 1. Official Qt mechanism - qtpaths knows where host-tools are located.
    for qtpaths in [format!("qtpaths{qt_major}"), "qtpaths".to_string()] {
        for var in ["QT_HOST_LIBEXECS", "QT_INSTALL_LIBEXECS"] {
            if let Ok(output) = std::process::Command::new(&qtpaths).arg("-query").arg(var).output() {
                if output.status.success() {
                    let dir = String::from_utf8_lossy(&output.stdout).trim().to_string();
                    if !dir.is_empty() {
                        candidates.push(format!("{dir}/rcc"));
                    }
                }
            }
        }
    }

    // 2. macOS Homebrew.
    for formula in [format!("qt@{qt_major}"), "qt".to_string()] {
        if let Ok(output) = std::process::Command::new("brew").arg("--prefix").arg(&formula).output() {
            if output.status.success() {
                let prefix = String::from_utf8_lossy(&output.stdout).trim().to_string();
                if !prefix.is_empty() {
                    candidates.push(format!("{prefix}/libexec/rcc"));
                    candidates.push(format!("{prefix}/bin/rcc"));
                }
            }
        }
    }

    // 3. Typical Linux paths - as a fallback option, if qtpaths is missing.
    for libdir in ["lib", "lib64"] {
        candidates.push(format!("/usr/{libdir}/qt{qt_major}/libexec/rcc"));
        candidates.push(format!("/usr/{libdir}/qt{qt_major}/bin/rcc"));
        candidates.push(format!("/usr/{libdir}/qt{qt_major}/rcc"));
    }

    // 4. Limited disk search - take the prefix from pkg-config and search
    // for the "rcc" file no deeper than 3 levels, instead of guessing the
    // specific subdirectory. Saves you when the distribution arranges files
    // somehow non-standardly (as in the cases from the Arch bug tracker).
    if let Ok(output) = std::process::Command::new("pkg-config")
        .arg("--variable=prefix")
        .arg(format!("Qt{qt_major}Core"))
        .output()
    {
        if output.status.success() {
            let prefix = String::from_utf8_lossy(&output.stdout).trim().to_string();
            if !prefix.is_empty() {
                find_file_shallow(std::path::Path::new(&prefix), "rcc", 3, &mut candidates);
            }
        }
    }

    candidates.push(format!("rcc-qt{qt_major}"));
    candidates.push("rcc".to_string());

    for candidate in &candidates {
        if let Ok(output) = std::process::Command::new(candidate).arg("--version").output() {
            if output.status.success() {
                let version_output = String::from_utf8_lossy(&output.stdout);
                if version_output.contains(&format!("{qt_major}.")) {
                    return Some(candidate.clone());
                }
            }
        }
    }

    None
}

fn qrc_files(project: &Project) -> Vec<PathBuf> {
    let mut found = Vec::new();
    if let Ok(entries) = std::fs::read_dir(&project.root) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.extension().and_then(|e| e.to_str()) == Some("qrc") {
                found.push(path);
            }
        }
    }
    found
}

fn run_rcc(
    qrc_files: &[PathBuf],
    build_dir: &std::path::Path,
    rcc_bin: &str,
    verbose: bool,
) -> Result<Vec<PathBuf>> {
    let mut generated = Vec::new();

    for qrc in qrc_files {
        let stem = qrc.file_stem().and_then(|s| s.to_str()).unwrap_or("resources");
        let rcc_out = build_dir.join(format!("qrc_{}.cpp", stem));

        let mut cmd = std::process::Command::new(rcc_bin);
        cmd.arg(qrc).arg("-o").arg(&rcc_out).arg("-name").arg(stem);

        crate::diagnostics::print_status("Rcc", &qrc.display().to_string());
        if verbose {
            println!("      $ {:?}", cmd);
        }

        let output = cmd.output().map_err(crate::error::BuildError::Io)?;
        if !output.status.success() {
            return Err(crate::error::BuildError::Compile(
                qrc.display().to_string(),
                String::from_utf8_lossy(&output.stderr).to_string(),
            ));
        }

        generated.push(rcc_out);
    }

    Ok(generated)
}

/// Build `project` (via [`build_project`]) and then execute the
/// resulting binary, forwarding its exit status.
///
/// # Errors
/// Propagates any error from [`build_project`]. Returns
/// [`crate::error::BuildError::CommandFailed`] if the binary itself
/// exits with a non-zero status.
pub fn run_project(project: &Project, release: bool, verbose: bool, dry_run: bool, incremental: bool) -> Result<()> {
    build_project(project, release, verbose, dry_run, incremental)?;

    if dry_run {
        println!("Dry run: skipping execution.");
        return Ok(());
    }

    let project_section = project.config.project.as_ref().ok_or_else(|| {
        crate::error::BuildError::Dependency {
            name: project.root.display().to_string(),
            reason: "cannot run: Smidr.toml has no [project] section".to_string(),
        }
    })?;
    let profile_dir = if release { "release" } else { "debug" };

    let output_name = project_section.output_name();
    let binary_name = if cfg!(windows) {
        format!("{}.exe", output_name)
    } else {
        output_name.to_string()
    };

    let binary_path = project.build_dir.join(profile_dir).join("bin").join(binary_name);

    println!();
    println!("Running: {}", binary_path.display());
    println!();
    let status = std::process::Command::new(&binary_path).status()?;

    if !status.success() {
        #[cfg(unix)]
        {
            use std::os::unix::process::ExitStatusExt;
            if let Some(signal) = status.signal() {
                return Err(crate::error::BuildError::Signaled {
                    cmd: binary_path.display().to_string(),
                    signal,
                });
            }
        }
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

pub fn rebuild_project(project: &Project, release: bool, verbose: bool, dry_run: bool) -> Result<()> {
    clean_project(project)?;
    build_project(project, release, verbose, dry_run, false)
}

/// Format project source and header files with clang-format.
///
/// # Errors
/// Returns [crate::error::BuildError::CompilerNotFound] if
/// clang-format is not on PATH. Returns
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
fn compiler_binary(kind: &crate::config::CompilerKind, language: &crate::config::Language) -> Result<&'static str> {
    use crate::config::{CompilerKind, Language};

    let candidates: &[&str] = match (kind, language) {
        (CompilerKind::Gcc, Language::C) => &["gcc"],
        (CompilerKind::Gcc, Language::Cpp) => &["g++"],
        (CompilerKind::Clang, Language::C) => &["clang"],
        (CompilerKind::Clang, Language::Cpp) => &["clang++"],
        (CompilerKind::Tcc, _) => &["tcc"],
        (CompilerKind::Auto, Language::C) => &["clang", "tcc", "cc", "gcc"],
        (CompilerKind::Auto, Language::Cpp) => &["clang++", "g++"],
    };

    for candidate in candidates {
        if command_exists(candidate) {
            return Ok(candidate);
        }
    }
    Err(crate::error::BuildError::CompilerNotFound(candidates.join(", ")))
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

pub fn lint_project(project: &Project) -> Result<()> {
    let project_section = project.config.project.as_ref().ok_or_else(|| {
        crate::error::BuildError::Dependency {
            name: project.root.display().to_string(),
            reason: "cannot lint: Smidr.toml has no [project] section".to_string(),
        }
    })?;

    let files = project.source_files()?;

    let clang_binary = match project_section.language {
        crate::config::Language::C => "clang",
        crate::config::Language::Cpp => "clang++",
    };

    let mut cmd = std::process::Command::new(clang_binary);
    cmd.arg("-fsyntax-only");
    cmd.args(&files);

    let status = cmd.status()?;
    if !status.success() {
        return Err(crate::error::BuildError::CommandFailed {
            cmd: clang_binary.to_string(),
            code: status.code(),
        });
    }

    println!("Linted {} file(s).", files.len());
    Ok(())
}

pub fn update_project() -> Result<()> {
    let status = std::process::Command::new("cargo")
        .args(["install", "smidr", "--force"])
        .status()?;

    if !status.success() {
        return Err(crate::error::BuildError::CompilerNotFound(
            "cargo".to_string(),
        ));
    }

    println!("Smidr updated successfully!");
    
    Ok(())
}

/// Validate that a project is correctly configured and ready to build,
/// without invoking the compiler on any source file. Unlike `lint`
/// (which asks "does the compiler accept this code"), `check` asks
/// "is this project set up correctly" - manifest sections, an available
/// compiler, and the presence of source/header files.
///
/// Every check runs even if an earlier one fails, so a single
/// invocation reports every problem at once rather than making the
/// user fix-and-rerun repeatedly.
pub fn check_project(project: &Project) -> Result<()> {
    let mut issues: Vec<String> = Vec::new();

    let project_section = match &project.config.project {
        Some(p) => {
            println!("[project] section: {} v{}", p.name, p.version);
            Some(p)
        }
        None => {
            issues.push("no [project] section in Smidr.toml".to_string());
            None
        }
    };

    let build_section = match &project.config.build {
        Some(b) => {
            println!("[build] section present");
            Some(b)
        }
        None => {
            issues.push("no [build] section in Smidr.toml".to_string());
            None
        }
    };

    match (project_section, build_section) {
        (Some(p), Some(b)) => match compiler_binary(&b.compiler, &p.language) {
            Ok(compiler) => println!("Compiler: {} (found on PATH)", compiler),
            Err(e) => issues.push(format!("no usable compiler found: {}", e)),
        },
        _ => issues.push(
            "skipped compiler check: [project] or [build] section missing".to_string(),
        ),
    }

    match project.source_files() {
        Ok(files) => println!("Source files: {}", files.len()),
        Err(e) => issues.push(format!("source files: {}", e)),
    }

    match project.header_files() {
        Ok(files) => println!("Header files: {}", files.len()),
        Err(_) => println!("  (no header files - fine if this project doesn't use any)"),
    }

    let dep_count = project.config.dependencies.len();
    println!("  Dependencies declared: {}", dep_count);
    println!();

    if issues.is_empty() {
        println!("Project looks ready to build.");
        Ok(())
    } else {
        eprintln!("Found {} issue(s):", issues.len());
        for issue in &issues {
            eprintln!("  ✗ {}", issue);
        }
        Err(crate::error::BuildError::Dependency {
            name: project.root.display().to_string(),
            reason: format!("{} issue(s) found - see above", issues.len()),
        })
    }
}

pub fn deps_project(project: &Project) -> Result<()> {
    if project.config.dependencies.is_empty() {
        println!("No dependencies found.");
        return Ok(());
    }

    println!("Dependencies:");
    for (name, spec) in &project.config.dependencies {
        match spec {
            crate::config::DependencySpec::Version(ver) => {
                println!("- {} ({})", name, ver);
            }
            crate::config::DependencySpec::Detailed {
                git,
                path,
                tag,
                branch,
                rev,
                ..
            } => {
                if let Some(path) = path {
                    println!("- {} (path: {})", name, path);
                } else if let Some(git) = git {
                    if let Some(tag) = tag {
                        println!("- {} (git: {}, tag: {})", name, git, tag);
                    } else if let Some(branch) = branch {
                        println!("- {} (git: {}, branch: {})", name, git, branch);
                    } else if let Some(rev) = rev {
                        println!("- {} (git: {}, rev: {})", name, git, rev);
                    } else {
                        println!("- {} (git: {})", name, git);
                    }
                } else {
                    println!("- {}", name);
                }
            }
        }
    }

    Ok(())
}
