# Contributing to Smidr

Thanks for considering a contribution. `Smidr` is a personal project maintained alongside my studies, so response times to issues and pull requests may be slow (see [If I've gone quiet](#if-ive-gone-quiet) below). It's also in early development, so the codebase and conventions may still shift - open an issue before starting on a larger change, to avoid duplicated or wasted work.

## Getting started

```sh
git clone https://github.com/Max-Mend/smidr
cd smidr
cargo build
cargo run -- new /tmp/smidr-test-project
```

## Project structure

See the [Architecture](README.md#architecture) section of the README for how the codebase is organized. In short: each module has one responsibility, and modules communicate through the types defined in `error.rs`, `config.rs`, and `toolchain.rs`. Before adding a new dependency-build-system or compiler, look at how `toolchain/cmake.rs` or the `compiler_binary` function in `builder.rs` are structured, and follow the same pattern.

## Making changes

- Run `cargo build` and `cargo clippy` before opening a pull request.
- Keep pull requests focused - one logical change per PR is easier to review than several unrelated ones bundled together.
- If you're fixing a bug, a minimal reproduction (a `smidr.toml` and/or `.c` file that triggers it) helps a lot.
- If you're adding a feature, briefly describe the motivation in the PR description - not every idea needs to land, and understanding the "why" speeds up review.

## If I've gone quiet

I'm a student, and coursework, tests, and exams sometimes take over completely for a week or two with little to no warning. If you don't hear back and it's been a while, it's almost certainly that - not disinterest in your issue or PR.

In the meantime:

- Feel free to dig into the code yourself. The architecture is modular by design (see [Project structure](#project-structure) above) - most fixes touch one file, and the existing modules (`toolchain/cmake.rs`, for example) are a reasonable template to follow for similar changes.
- If others have hit the same issue, discussing it in the issue thread or [GitHub Discussions](https://github.com/Max-Mend/smidr/discussions) may get you an answer faster than waiting on me alone.
- Pull requests that fix a clear bug are usually easy to merge once I'm back, even if I couldn't review them right away.

## Reporting issues

When reporting a bug, please include:

- your OS and Rust version (`rustc --version`)
- the exact command you ran
- the full error output
- your `smidr.toml`, if relevant

## Security

Please do not open a public issue for security vulnerabilities. See [SECURITY.md](SECURITY.md) for how to report them privately.

## License

By contributing, you agree that your contributions will be licensed under the same terms as the project - MIT OR Apache-2.0. See [LICENSE-MIT](LICENSE-MIT) and [LICENSE-APACHE](LICENSE-APACHE).
