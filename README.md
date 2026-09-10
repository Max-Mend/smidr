<h1 align=center>Smidr</h1>

<div align="center">
  <h3 align=left>Support the Project</h3>
  <h5 style="font-size: 15px; font-weight: 500;">If you find Smidr helpful and want to support my work and studies, you can buy me a coffee!</h5>
  <a href="https://ko-fi.com/maxmend"><img src="https://img.shields.io/badge/Donate-Ko--Fi-F16061?style=for-the-badge&logo=ko-fi&logoColor=white&labelColor=101418" alt="Ko-Fi"></a>
</div>

---

<div align=center>

[![License](https://img.shields.io/badge/License-MIT%20OR%20Apache--2.0-02569B?style=for-the-badge)](#license)
[![Rust](https://img.shields.io/badge/Rust-000000?style=for-the-badge&logo=rust&logoColor=white)](https://www.rust-lang.org/)

</div>

Smidr is a `cargo`- inspired build tool for C projects, meant to bridge dependencies across different build systems into a single build.

## Why

Working with C projects usually means hand-writing a Makefile, CMakeLists.txt, or invoking the compiler directly. `Smidr` handles that:

- scaffolds a new project
- discovers and compiles `.c` sources
- links the result into a binary
- keeps project configuration in a plain `Smidr.toml`, not a bespoke build script
- is not tied to a specific compiler (clang, tcc, gcc, or the system `cc`)

## Installation

Requires Rust 1.85 or newer (this project uses the 2024 edition). Tested with Rust 1.96.0.

```sh
cargo install smidr
```

Or build from source:

```sh
git clone https://github.com/Max-Mend/smidr
cd smidr
cargo install --path .
```

## Usage

```sh
smidr new hello
cd hello
smidr build
smidr run
```

```console
$ smidr new demo
Created project: demo

$ cd demo && smidr run
Using compiler: cc
Running: target/bin/demo
Hello, World!
```

`smidr new hello` scaffolds:

```
hello/
├── Smidr.toml
├── .gitignore
├── include/
└── src/
    └── main.c
```

### Commands

| Command | Description |
| --- | --- |
| `smidr new <name>` | Scaffold a new project |
| `smidr build` | Compile the project into `target/bin/` |
| `smidr run` | Compile and run the resulting binary |

## Configuration

Project metadata and build settings live in `Smidr.toml`:

```toml
[project]
name = "hello"
version = "0.1.0"
authors = []
authors_email = []

[build]
compiler = "auto"      # auto | clang | tcc | gcc
warnings = "standard"  # none | standard | strict
cflags = []

[dependencies]
<<<<<<< HEAD
# external C libraries - see Roadmap
=======
# system library
zlib = "1.3"
# git, latest stable tag
raylib = { git = "https://github.com/raysan5/raylib" }
# local project
mymath = { path = "../mymath" }
>>>>>>> 23f4bb8 (feat: git rev support, global cache for cloned dependencies (Cargo-style))
```

| `[build]` field | Values | Description |
| --- | --- | --- |
| `compiler` | `auto`, `clang`, `tcc`, `gcc` | `auto` tries, in order: `clang`, `tcc`, the system `cc`, then `gcc` |
| `warnings` | `none`, `standard`, `strict` | `standard` adds `-Wall -Wextra`; `strict` adds `-Werror -Wpedantic` |
| `cflags` | list of strings | additional flags passed to the compiler |

### A note on `[dependencies]`

<<<<<<< HEAD
The `[dependencies]` section is parsed but not yet wired into the build - see [Roadmap](#roadmap). Once it is, be aware that `build_system = "custom"` runs arbitrary shell commands defined in `build_commands`. Only use a `Smidr.toml` from a source you trust, the same way you would with any shell script.

## Examples

- [Ricochet](https://github.com/Max-Mend/ricochet) - a DVD-logo-style terminal screensaver, built entirely with `Smidr` using only the C standard library.
=======
- **A version string** (`zlib = "1.3"`) - resolved from a local header search, then `pkg-config`
- **`path`** - a local directory. If it has its own `Smidr.toml`, it's built recursively with Smidr; otherwise Smidr detects and drives its CMake/Meson/Make build
- **`git`** - cloned at a pinned `tag`, `branch`, or `rev` (a specific commit), or the latest stable release tag if none of the three is given, then resolved the same way as `path`. Only one of `tag`/`branch`/`rev` may be set at a time.

```toml
# latest stable tag
raylib = { git = "https://github.com/raysan5/raylib" }

# pinned tag
raylib = { git = "https://github.com/raysan5/raylib", tag = "5.5" }

# tracks a branch
raylib = { git = "https://github.com/raysan5/raylib", branch = "master" }

# pinned commit
raylib = { git = "https://github.com/raysan5/raylib", rev = "a1b2c3d" }
```

Cloned git dependencies are cached globally at `$XDG_CACHE_HOME/smidr/git`
(or `~/.cache/smidr/git`), shared across every Smidr project on the
machine - the same dependency pinned to the same `tag`/`branch`/`rev` is
only ever fetched from the network once.

> **Note:** the cache is keyed by URL + resolved ref and is never
> refreshed automatically. This is exact for `tag` and `rev` (both are
> immutable), but a `branch` dependency stays pinned to whichever commit
> was on that branch the first time it was resolved on your machine,
> even after the branch moves forward upstream. If you need the latest
> commit on a tracked branch, clear the cache directory (or the specific
> entry under it) to force a fresh clone.

> `build_system = "custom"` runs arbitrary shell commands from `build_commands` in `Smidr.toml`. Only use a `Smidr.toml` from a source you trust, the same way you would with any shell script.

### Workspaces

A `Smidr.toml` can also organize several projects:

```toml
[workspace]
members = ["core", "app"]
```

This works whether or not the root itself has a `[project]` section - a pure organizational root just builds its members; a root with its own `[project]` builds itself too.

### Custom source directories

```toml
[paths]
src_dir = "sources"    # override the default "src"
include = "headers"     # override the default "include"
core = "core"            # any extra name compiles alongside src_dir
platform = "platform"
```

## Examples

- [Ricochet](https://github.com/Max-Mend/ricochet) - a DVD-logo-style terminal screensaver, built entirely with Smidr using only the C standard library. (Ricochet was built with Smidr **0.1.0**)
>>>>>>> 23f4bb8 (feat: git rev support, global cache for cloned dependencies (Cargo-style))

## Architecture

```
src/
├── main.rs         entry point, CLI dispatch
├── cli.rs           command definitions (clap)
├── config.rs        smidr.toml types and (de)serialization
├── project.rs        project model: init, load, source discovery
├── builder.rs         compilation and linking
├── resolver.rs        dependency source resolution (git or local path)
├── compile_db.rs       compile_commands.json generation, for clangd and other LSPs
├── error.rs           the crate's error type
└── toolchain/          build-system abstraction for dependencies
    ├── cmake.rs
    ├── meson.rs
    ├── make.rs
    └── custom.rs
```

Each external process (the compiler, `cmake`, `meson`) is invoked behind a dedicated, isolated layer. `Smidr` itself is not tied to any single build tool.

## Roadmap

- [x] `smidr new` - project scaffolding
- [x] `smidr build` / `smidr run` - compile and run
- [x] Compiler-agnostic builds, with auto-detection (clang, tcc, gcc)
- [ ] `compile_commands.json` generation - implemented, not yet wired into `build`
- [ ] Local (`path =`) dependencies - resolver implemented, not yet wired into `build`
- [ ] Build-system auto-detection for dependencies (CMake, Meson, Make) - implemented, not yet wired into `build`
- [ ] Git dependencies
- [ ] Linking against CMake/Meson-built libraries (`pkg-config` resolution)
- [ ] MSVC support

## Contributing

This project is in early development, maintained alongside my studies - response to issues and pull requests can be slow at times (exam periods especially). See [CONTRIBUTING.md](CONTRIBUTING.md) for what to do in the meantime if you run into a bug and don't hear back right away. Issues and pull requests are welcome regardless - for larger changes, open an issue first to discuss the approach.

## Security

Please do not open a public issue for security vulnerabilities - see [SECURITY.md](SECURITY.md) for how to report them privately.

## License

Licensed under either of

- Apache License, Version 2.0 ([LICENSE-APACHE](LICENSE-APACHE))
- MIT license ([LICENSE-MIT](LICENSE-MIT))

at your option.

### Contribution

Unless you explicitly state otherwise, any contribution intentionally submitted for inclusion in the work by you, as defined in the Apache-2.0 license, shall be dual licensed as above, without any additional terms or conditions.
