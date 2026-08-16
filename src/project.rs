// SPDX-License-Identifier: MIT OR Apache-2.0
// Copyright (c) 2026 Max-Mend
// This file is part of smidr: https://github.com/Max-Mend/smidr

//! The in-memory model of a `smidr` project.
//!
//! [`Project`] is a read-only view of what already exists (or should
//! exist) on disk — [`Project::load`] reads `smidr.toml` and computes
//! standard paths, [`Project::source_files`] reads `src/`. Neither
//! creates or modifies any files. [`Project::init`] is the one exception:
//! it's the write side, used by `smidr new` to scaffold a brand new
//! project from scratch. All other file writes (compiled objects, the
//! linked binary, dependency installs) live in [`crate::builder`] and
//! [`crate::toolchain`], not here.

use crate::config::{BuildSection, DependencySpec, ManifestConfig, ProjectSection};
use crate::error::{BuildError, Result};
use crate::toolchain::BuildOutput;
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

/// The `src/main.c` contents written by `smidr new`.
const MAIN_C_TEMPLATE: &str =
    "#include <stdio.h>\n\nint main() {\n    printf(\"Hello, World!\\n\");\n    return 0;\n}\n";

/// A loaded (or freshly initialized) `smidr` project.
///
/// This is what [`crate::builder::build_project`] and
/// [`crate::builder::run_project`] take as input, and what accumulates
/// dependency build results in `resolved_deps` as they're resolved.
pub struct Project {
    /// The project's root directory (where `smidr.toml` lives).
    pub root: PathBuf,
    /// The fully parsed `smidr.toml` — `[project]`, `[build]`, and
    /// `[dependencies]`.
    pub config: ManifestConfig,

    /// `root/src` — where `.c` source files are discovered.
    pub src_dir: PathBuf,
    /// `root/target` — where object files and the final binary are written.
    pub build_dir: PathBuf,
    /// `root/target/deps` — where dependency install prefixes live (see
    /// [`Project::dep_prefix`]).
    pub install_dir: PathBuf,

    /// Build results for each resolved dependency, keyed by dependency
    /// name. Empty right after [`Project::load`] — populated
    /// incrementally as dependencies are resolved and built.
    pub resolved_deps: Vec<(String, BuildOutput)>,
}

impl Project {
    /// Load an existing project from `project_dir`: parse `smidr.toml`
    /// and compute the standard `src/`, `target/`, and `target/deps/`
    /// paths relative to it.
    ///
    /// # Errors
    /// Propagates [`BuildError::ManifestNotFound`] or [`BuildError::TomlDe`]
    /// from [`ManifestConfig::load`] if `smidr.toml` is missing or invalid.
    pub fn load(project_dir: &Path) -> Result<Self> {
        let config = ManifestConfig::load(project_dir)?;

        Ok(Self {
            root: project_dir.to_path_buf(),
            src_dir: project_dir.join("src"),
            build_dir: project_dir.join("target"),
            install_dir: project_dir.join("target/deps"),
            config,
            resolved_deps: Vec::new(),
        })
    }

    /// Scaffold a brand new project named `name` in the current directory:
    /// creates `src/`, `include/`, a starter `src/main.c`, a generated
    /// `smidr.toml`, and a `.gitignore`.
    ///
    /// # Errors
    /// Returns [`BuildError::InvalidProjectName`] if `name` is empty or
    /// contains a path separator or `.`/`..` (this also guards against
    /// writing outside the intended directory). Returns
    /// [`BuildError::ProjectAlreadyExists`] if `name` already exists on
    /// disk.
    pub fn init(name: &str) -> Result<()> {
        if name.is_empty()
            || name.contains('/')
            || name.contains('\\')
            || name == "."
            || name == ".."
        {
            return Err(BuildError::InvalidProjectName(name.to_string()));
        }

        let root = PathBuf::from(name);
        if root.exists() {
            return Err(BuildError::ProjectAlreadyExists(root));
        }

        std::fs::create_dir_all(root.join("src"))?;
        std::fs::create_dir_all(root.join("include"))?;

        std::fs::write(root.join("src/main.c"), MAIN_C_TEMPLATE)?;

        let config = ManifestConfig {
            project: ProjectSection {
                name: name.to_string(),
                version: "0.1.0".to_string(),
                authors: Vec::new(),
                authors_email: Vec::new(),
                c_standard: None,
            },
            build: BuildSection {
                compiler: Default::default(),
                warnings: Default::default(),
                cflags: Vec::new(),
            },
            dependencies: BTreeMap::new(),
        };
        std::fs::write(root.join("smidr.toml"), config.to_toml_string()?)?;

        std::fs::write(root.join(".gitignore"), "target/\ncompile_commands.json\n")?;

        println!("✅ Створено проєкт: {}", name);
        Ok(())
    }

    /// Find every `.c` file directly inside `src_dir` (non-recursive).
    ///
    /// # Errors
    /// Returns [`BuildError::NoSourceFiles`] if `src/` exists but contains
    /// no `.c` files — this is treated as a build error rather than
    /// silently producing an empty binary.
    pub fn source_files(&self) -> Result<Vec<PathBuf>> {
        let mut sources = Vec::new();

        let entries = std::fs::read_dir(&self.src_dir).map_err(BuildError::Io)?;
        for entry in entries {
            let entry = entry.map_err(BuildError::Io)?;
            let path = entry.path();
            if path.is_file() && path.extension().and_then(|s| s.to_str()) == Some("c") {
                sources.push(path);
            }
        }

        if sources.is_empty() {
            return Err(BuildError::NoSourceFiles);
        }

        Ok(sources)
    }

    /// The install prefix for a given dependency — where its build system
    /// (CMake, Meson, Make, or a custom script) is told to install its
    /// `include/` and `lib/` output.
    pub fn dep_prefix(&self, dep_name: &str) -> PathBuf {
        self.install_dir.join(dep_name)
    }
}
