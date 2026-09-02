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
            project: Some(ProjectSection {
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
            }),
            
            build: Some(BuildSection {
                compiler: Default::default(),
                cflags: Vec::new(),
                libs: Vec::new(),
                linker_flags: Vec::new(),
            }),
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
        if let Some(project_section) = &self.config.project {
            if sources.is_empty() && project_section.project_type == ProjectType::Binary {
                return Err(BuildError::NoSourceFiles);
            }
        }
        Ok(sources)
    }

    pub fn header_files(&self) -> Result<Vec<PathBuf>> {
        let headers = self.collect_files(&[&self.root.join("include")], &["h", "hpp", "hh"])?;
        if let Some(project_section) = &self.config.project {
            if headers.is_empty() && project_section.project_type == ProjectType::StaticLibrary {
                return Err(BuildError::NoHeaderFiles);
            }
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
    pub fn resolve_dependencies(&mut self, is_release: bool) -> Result<()> {
        let dependencies = self.config.dependencies.clone();
        for (name, spec) in &dependencies {
            match resolver::resolve(name, spec, &self.root)? {
                SourceLocation::System { .. } => {
                    let lib_info = resolver::resolve_system_lib(name)?;
                    self.resolved_deps.push((
                        name.to_string(),
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
                SourceLocation::Path(dep_root) => {
                    self.build_and_register_dep(name, spec, &dep_root, is_release)?;
                }
                SourceLocation::Git { url, tag } => {
                    let dest = self.build_dir.join("deps-src").join(name);
                    let dep_root = resolver::resolve_git(name, &url, &tag, &dest)?;
                    self.build_and_register_dep(name, spec, &dep_root, is_release)?;
                }
            }
        }
        Ok(())
    }

    /// Build a dependency found at `dep_root` (a `path` on disk, or a
    /// freshly-cloned `git` checkout) and register the result in
    /// `resolved_deps`. Shared between `Path` and `Git` sources, which
    /// differ only in how `dep_root` was found.
    fn build_and_register_dep(
        &mut self,
        name: &str,
        spec: &crate::config::DependencySpec,
        dep_root: &Path,
        is_release: bool,
    ) -> Result<()> {
        if dep_root.join("Smidr.toml").exists() {
            let dep_project = crate::project::Project::load(dep_root)?;
            crate::builder::build_project(&dep_project, is_release)?;

            let dep_include = dep_root.join("include");
            let profile_dir = if is_release { "target/release" } else { "target/debug" };
            let lib_filename = format!("lib{}.a", name);
            let dep_original_lib_path = dep_root.join(profile_dir).join("bin").join(&lib_filename);

            let deps_cache_dir = self.build_dir.join(profile_dir).join("deps");
            std::fs::create_dir_all(&deps_cache_dir)?;
            let dep_cached_lib_path = deps_cache_dir.join(&lib_filename);

            let needs_update = match (
                std::fs::metadata(&dep_cached_lib_path),
                std::fs::metadata(&dep_original_lib_path),
            ) {
                (Ok(cached), Ok(orig)) => cached.modified().unwrap() < orig.modified().unwrap(),
                _ => true,
            };

            if needs_update {
                std::fs::copy(&dep_original_lib_path, &dep_cached_lib_path)?;
            }

            self.resolved_deps.push((
                name.to_string(),
                BuildOutput {
                    include_dirs: vec![dep_include],
                    lib_dirs: vec![deps_cache_dir],
                    libs: vec![name.to_string()],
                },
            ));
        } else {
            let build_system = match spec {
                crate::config::DependencySpec::Detailed { build_system, .. } => build_system,
                crate::config::DependencySpec::Version(_) => &crate::config::BuildSystemKind::Auto,
            };
            let builder = crate::toolchain::resolve_builder(name, build_system, dep_root, spec)?;
            let output = builder.build(dep_root, &self.dep_prefix(name))?;
            self.resolved_deps.push((name.to_string(), output));
        }
        Ok(())
    }

    pub fn add_dependency(&mut self, name: &str) -> Result<()> {
        resolver::resolve_system_lib(name)?;
        self.config.dependencies.insert(
            name.to_string(),
            crate::config::DependencySpec::Version("*".to_string()),
        );

        std::fs::write(self.root.join("Smidr.toml"), self.config.to_toml_string()?)?;

        println!("Added dependency: {}", name);
        Ok(())
    }

    pub fn remove_dependency(&mut self, name: &str) -> Result<()> {
        self.config.dependencies.remove(name);
        std::fs::write(self.root.join("Smidr.toml"), self.config.to_toml_string()?)?;

        println!("Removed dependency: {}", name);
        Ok(())
    }
}
