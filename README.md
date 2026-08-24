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
# external C libraries - see Roadmap
```

| `[build]` field | Values | Description |
| --- | --- | --- |
| `compiler` | `auto`, `clang`, `tcc`, `gcc` | `auto` tries, in order: `clang`, `tcc`, the system `cc`, then `gcc` |
| `warnings` | `none`, `standard`, `strict` | `standard` adds `-Wall -Wextra`; `strict` adds `-Werror -Wpedantic` |
| `cflags` | list of strings | additional flags passed to the compiler |

### A note on `[dependencies]`

The `[dependencies]` section is parsed but not yet wired into the build - see [Roadmap](#roadmap). Once it is, be aware that `build_system = "custom"` runs arbitrary shell commands defined in `build_commands`. Only use a `Smidr.toml` from a source you trust, the same way you would with any shell script.

## Examples

- [Ricochet](https://github.com/Max-Mend/ricochet) - a DVD-logo-style terminal screensaver, built entirely with `Smidr` using only the C standard library.

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

### Stars

<a>href="https://www.star-history.com/?repos=Max-Mend%2Fsmidr&type=date&logscale=&legend=top-left"</a>
