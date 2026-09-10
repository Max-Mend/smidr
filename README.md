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

## What is Smidr?

Smidr is a `cargo`-inspired build tool for C and C++. Instead of hand-writing a Makefile or a `CMakeLists.txt`, you describe your project in one `Smidr.toml` file, and Smidr scaffolds it, compiles it, resolves its dependencies, and links it - across binaries, static libraries, and shared libraries.

It's not a replacement for CMake or Meson in large, established codebases - it's for the everyday case: you want to start a C/C++ project, add a couple of libraries, and build it, without maintaining a build script by hand.

## Why

- **One command to start**: `smidr new` scaffolds a working project, no boilerplate to copy
- **Dependencies without ceremony**: a system library, a local project, or a git repository are all just an entry in `[dependencies]`
- **Not tied to one compiler**: works with `clang`, `gcc`, or `tcc`, on Linux, macOS, or Windows (with MinGW/Clang)
- **Bridges other build systems**: a dependency that itself uses CMake or Meson is built through Smidr transparently
- **Plain TOML, not a scripting language**: `Smidr.toml` is data, not a program to debug

## Quick start (1 minute)

```sh
cargo install smidr
smidr new hello
cd hello
smidr run
```

```console
$ smidr run
Using compiler: clang
Running: target/debug/bin/hello
Hello, World!
```

That's it - `smidr new` scaffolds a project, `smidr run` compiles and executes it.

## Installation

Requires Rust 1.85 or newer (Smidr uses the 2024 edition). Tested with Rust 1.96.0.

```sh
cargo install smidr
```

Or build from source:

```sh
git clone https://github.com/Max-Mend/smidr
cd smidr
cargo install --path .
```

### Linux / macOS

Just needs a C/C++ compiler already on your system - `clang`, `gcc`, or `tcc`. Most Linux distributions and macOS (via Xcode Command Line Tools) already have one.

### Windows

Smidr itself runs natively on Windows, but it needs a GCC or Clang toolchain in `PATH` to actually compile anything - Windows has no compiler out of the box.

The simplest way to get one:

1. Install [MSYS2](https://www.msys2.org/)
2. Open the **MSYS2 MinGW 64-bit** terminal and run:
   `pacman -S mingw-w64-x86_64-gcc`
3. Add `C:\msys64\mingw64\bin` to your system `PATH`
4. Open a **new** terminal and confirm with `gcc --version`

Binaries are written as `.exe`, static libraries as `.lib`, and shared libraries as `.dll` automatically. Native MSVC (`cl.exe`) support isn't implemented yet - see the [Roadmap](#roadmap).

## Usage

```sh
smidr new hello              # scaffold a binary project
smidr new mylib --lib        # scaffold a static library
smidr new mylib --type dynamic --std cpp20   # a C++20 shared library
smidr build                  # compile
smidr build --release        # compile with optimizations
smidr run                    # compile and run
smidr fmt                    # format sources with clang-format
smidr lint                   # check syntax without compiling
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
| `smidr new <name>` | Scaffold a new project (`--lib`, `--type dynamic`, `--std <standard>`) |
| `smidr build` | Compile the project (`--release`, `--verbose`, `--dry-run`) |
| `smidr run` | Compile and run the resulting binary |
| `smidr rebuild` | Clean, then compile from scratch |
| `smidr clean` | Remove the `target/` build directory |
| `smidr fmt` | Format source and header files with `clang-format` |
| `smidr lint` | Check source files for syntax errors without compiling |
| `smidr add <name>` | Add a dependency to `Smidr.toml` |
| `smidr rm <name>` | Remove a dependency from `Smidr.toml` |
| `smidr update` | Update Smidr itself to the latest version |

## Configuration

```toml
[project]
name = "hello"
version = "0.1.0"
type = "bin"           # bin | static | dynamic
language = "c"          # c | cpp
c_standard = "c17"

[build]
compiler = "auto"       # auto | clang | tcc | gcc
cflags = []
libs = []
linker_flags = []

[dependencies]
zlib = "1.3"                                    # system library
raylib = { git = "https://github.com/raysan5/raylib" }   # git, latest stable tag
mymath = { path = "../mymath" }                  # local project
```

### Dependencies

A dependency can come from three places:

- **A version string** (`zlib = "1.3"`) - resolved from a local header search, then `pkg-config`
- **`path`** - a local directory. If it has its own `Smidr.toml`, it's built recursively with Smidr; otherwise Smidr detects and drives its CMake/Meson/Make build
- **`git`** - cloned at a pinned `tag`, `branch`, or `rev` (a specific commit), or the latest stable release tag if none of the three is given, then resolved the same way as `path`. Only one of `tag`/`branch`/`rev` may be set at a time.

```toml
raylib = { git = "https://github.com/raysan5/raylib" }                       # latest stable tag
raylib = { git = "https://github.com/raysan5/raylib", tag = "5.5" }          # pinned tag
raylib = { git = "https://github.com/raysan5/raylib", branch = "master" }    # tracks a branch
raylib = { git = "https://github.com/raysan5/raylib", rev = "a1b2c3d" }      # pinned commit
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

- [Ricochet](https://github.com/Max-Mend/ricochet) - a DVD-logo-style terminal screensaver, built entirely with Smidr using only the C standard library. (Ricochet was built with Smidr **0.1.0**.)

## FAQ / Troubleshooting

**`Error: Compiler 'clang, tcc, cc, gcc' not found.`**
No C/C++ compiler is on your `PATH`. On Linux/macOS, install one via your package manager (`apt install clang`, `xcode-select --install`, etc.). On Windows, see [Installation](#windows).

**`Error: Dependency '<name>' failed: not found locally or via pkg-config`**
The system library isn't installed, or has no `pkg-config` entry. Install it via your package manager, or use a `path`/`git` dependency instead.

**`cannot build: Smidr.toml has no [project] section (this is a workspace root)`**
You ran `smidr run` from a workspace root that only organizes other projects. Run from inside a specific member directory instead.

**`No .h files found in include/`**
A `static` or `dynamic` library project needs at least one header in `include/` (or your `[paths] include` directory) - otherwise nothing else can use it.

**A `path`/`git` dependency isn't being picked up.**
Check that its `Smidr.toml` is valid on its own (`cd` into it and run `smidr build` directly) - a broken dependency manifest fails the same way a broken top-level one would.

**How do I pass custom CMake flags?**
The built-in `build_system = "cmake"` uses fixed flags. For custom `-D...` options, use `build_system = "custom"` with your own `build_commands` and `$SMIDR_PREFIX`.

**Does `smidr lint` work without a full build?**
Yes. It runs `clang`/`clang++ -fsyntax-only` and does not produce binaries.

## Roadmap

- [x] Project scaffolding, compiler-agnostic builds (clang, tcc, gcc)
- [x] C++ support (compiler selection, standards, scaffolding)
- [x] System, `path`, and `git` dependencies; CMake/Meson/Make bridging
- [x] Workspaces, custom source directories, build profiles
- [x] Windows support via MinGW/Clang
- [ ] Native MSVC support
- [ ] A modular system for optional, downloadable capabilities (e.g. `smidr module add <name>`) for things like kernel/bare-metal targets, kept out of the base install

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