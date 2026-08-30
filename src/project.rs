// SPDX-License-Identifier: MIT OR Apache-2.0
// Copyright (c) 2026 Max-Mend
// This file is part of smidr: https://github.com/Max-Mend/smidr

//! The in-memory model of a `smidr` project.
//!
//! [`Project`] is a read-only view of what already exists (or should
//! exist) on disk - [`Project::load`] reads `Smidr.toml` and computes
//! standard paths, [`Project::source_files`] reads `src/`. Neither
//! creates or modifies any files. [`Project::init`] is the one exception:
//! it's the write side, used by `smidr new` to scaffold a brand new
//! project from scratch. All other file writes (compiled objects, the
//! linked binary, dependency installs) live in [`crate::builder`] and
//! [`crate::toolchain`], not here.

use crate::config::{BuildSection, CStandard, Language, ManifestConfig, ProjectSection, ProjectType};
use crate::error::{BuildError, Result};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use crate::resolver::{self, SourceLocation};
use crate::toolchain::BuildOutput;

const GITIGNORE_TEMPLATE: &str = include_str!("../templates/.gitignore");
const MAIN_C_TEMPLATE: &str = include_str!("../templates/main.c");
const LIB_C_TEMPLATE: &str = include_str!("../templates/lib.c");
const LIB_H_TEMPLATE: &str = include_str!("../templates/lib.h");
const MAIN_CPP_TEMPLATE: &str = include_str!("../templates/main.cpp");
const LIB_CPP_TEMPLATE: &str = include_str!("../templates/lib.cpp");
const LIB_HPP_TEMPLATE: &str = include_str!("../templates/lib.hpp");

/// A loaded (or freshly initialized) `smidr` project.
///
/// This is what [`crate::builder::build_project`] and
/// [`crate::builder::run_project`] take as input, and what accumulates
/// dependency build results in `resolved_deps` as they're resolved.
pub struct Project {
    /// The project's root directory (where `Smidr.toml` lives).
    pub root: PathBuf,
    /// The fully parsed `Smidr.toml` - `[project]`, `[build]`, and
    /// `[dependencies]`.
    pub config: ManifestConfig,

    /// `root/src` - where `.c` source files are discovered.
    pub src_dir: PathBuf,
    /// `root/target` - where object files and the final binary are written.
    pub build_dir: PathBuf,
    /// `root/target/deps` - where dependency install prefixes live (see
    /// [`Project::dep_prefix`]).
    pub install_dir: PathBuf,

    /// Build results for each resolved dependency, keyed by dependency
    /// name. Empty right after [`Project::load`] - populated
    /// incrementally as dependencies are resolved and built.
    pub resolved_deps: Vec<(String, BuildOutput)>,
}

impl Project {
    /// Load an existing project from `project_dir`: parse `Smidr.toml`
    /// and compute the standard `src/`, `target/`, and `target/deps/`
    /// paths relative to it.
    ///
    /// # Errors
    /// Propagates [`BuildError::ManifestNotFound`] or [`BuildError::TomlDe`]
    /// from [`ManifestConfig::load`] if `Smidr.toml` is missing or invalid.
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
    /// `Smidr.toml`, and a `.gitignore`.
    ///
    /// # Errors
    /// Returns [`BuildError::InvalidProjectName`] if `name` is empty or
    /// contains a path separator or `.`/`..` (this also guards against
    /// writing outside the intended directory). Returns
    /// [`BuildError::ProjectAlreadyExists`] if `name` already exists on
    /// disk.
    pub fn init(
        name: &str,
        project_type: ProjectType,
        c_standard: Option<CStandard>,
    ) -> Result<()> {
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

        match project_type {
            ProjectType::Binary => {
                std::fs::write(root.join("src/main.c"), MAIN_C_TEMPLATE)?;
            }
            ProjectType::StaticLibrary | ProjectType::SharedLibrary => {
                std::fs::write(root.join("src/lib.c"), LIB_C_TEMPLATE)?;
                std::fs::write(root.join("include/lib.h"), LIB_H_TEMPLATE)?;
            }
        }        

        let config = ManifestConfig {
            project: ProjectSection {
                name: name.to_string(),
                version: "0.1.0".to_string(),
                authors: Vec::new(),
                authors_email: Vec::new(),
                description: None,
                license: None,
                project_type,
                language: Language::C,
                c_standard: c_standard.unwrap_or_default(),
                cpp_standard: None,
                output_name: None,
            },
            
            build: BuildSection {
                compiler: Default::default(),
                cflags: Vec::new(),
                libs: Vec::new(),
                linker_flags: Vec::new(),
            },
            dependencies: BTreeMap::new(),
            workspace: None,
            profile: Default::default(),
            extra_bins: Vec::new(),
        };
        std::fs::write(root.join("Smidr.toml"), config.to_toml_string()?)?;
        std::fs::write(root.join(".gitignore"), GITIGNORE_TEMPLATE)?;

        println!("Created project: {}", name);
        Ok(())
    }

    /// Finds all source and header files in `dirs` matching `extensions` (recursively).
    ///
    /// # Errors
    /// Returns [`BuildError::Io`] if reading directory contents fails.
    fn collect_files(&self, dirs: &[&Path], extensions: &[&str]) -> Result<Vec<PathBuf>> {
        let mut files = Vec::new();
        for dir in dirs {
            if !dir.exists() {
                continue;
            }
            Self::collect_files_recursive(dir, extensions, &mut files)?;
        }
        Ok(files)
    }

    fn collect_files_recursive(dir: &Path, extensions: &[&str], out: &mut Vec<PathBuf>) -> Result<()> {
        for entry in std::fs::read_dir(dir).map_err(BuildError::Io)? {
            let entry = entry.map_err(BuildError::Io)?;
            let path = entry.path();

            if path.is_dir() {
                Self::collect_files_recursive(&path, extensions, out)?;
            } else if let Some(ext) = path.extension().and_then(|s| s.to_str()) {
                if extensions.contains(&ext) {
                    out.push(path);
                }
            }
        }
        Ok(())
    }

    /// Finds source files in `src_dir`.
    ///
    /// # Errors
    /// Returns [`BuildError::NoSourceFiles`] if `src/` contains no matching source files.
    pub fn source_files(&self) -> Result<Vec<PathBuf>> {
        let sources = self.collect_files(&[&self.src_dir], &["c", "cpp", "cc", "cxx"])?;
        if sources.is_empty() {
            return Err(BuildError::NoSourceFiles);
        }
        Ok(sources)
    }

    pub fn header_files(&self) -> Result<Vec<PathBuf>> {
        let headers = self.collect_files(&[&self.root.join("include")], &["h", "hpp", "hh"])?;
        if headers.is_empty() && self.config.project.project_type == ProjectType::StaticLibrary {
            return Err(BuildError::NoHeaderFiles);
        }
        Ok(headers)
    }

    /// Finds all source and header files in `src_dir` and `include` for formatting.
    pub fn formattable_files(&self) -> Result<Vec<PathBuf>> {
        let include_dir = self.root.join("include");
        self.collect_files(
            &[&self.src_dir, &include_dir],
            &["c", "h", "cpp", "hpp", "cc", "hxx", "cxx", "hh"],
        )
    }

    /// The install prefix for a given dependency - where its build system
    /// (CMake, Meson, Make, or a custom script) is told to install its
    /// `include/` and `lib/` output.
    pub fn dep_prefix(&self, dep_name: &str) -> PathBuf {
        self.install_dir.join(dep_name)
    }

    /// Resolve every entry in `[dependencies]` and populate `resolved_deps`.
    ///
    /// For now, only `Version` (system library) specs are handled -
    /// `path`/`git` wiring into an actual build comes later.
    pub fn resolve_dependencies(&mut self) -> Result<()> {
        for (name, spec) in &self.config.dependencies {
            match resolver::resolve(name, spec, &self.root)? {
                SourceLocation::System { .. } => {
                    let lib_info = resolver::resolve_system_lib(name)?;
                    self.resolved_deps.push((
                        name.clone(),
                        BuildOutput {
                            include_dirs: lib_info.cflags.iter()
                                .filter_map(|f| f.strip_prefix("-I").map(PathBuf::from))
                                .collect(),
                            lib_dirs: Vec::new(),
                            libs: lib_info.libs.iter()
                                .filter_map(|f| f.strip_prefix("-l").map(String::from))
                                .collect(),
                        },
                    ));
                }
                SourceLocation::Path(_) => {
                    // TODO: recursive build of path dependency
                }
                SourceLocation::Git { .. } => {
                    // git not supported yet
                }
            }
        }
        Ok(())
    }
}
