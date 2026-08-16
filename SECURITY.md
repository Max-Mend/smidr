# Security Policy

## Supported Versions

`smidr` is in early (MVP) development. Only the latest version on the `main` branch is currently supported with security fixes.

| Version | Supported |
| --- | --- |
| latest (main) | :white_check_mark: |
| older releases | :x: |

## Reporting a Vulnerability

Please do **not** open a public GitHub issue for security vulnerabilities.

Instead, report it privately through [GitHub Security Advisories](https://github.com/Max-Mend/smidr/security/advisories/new) for this repository. This lets us investigate and prepare a fix before the issue is publicly disclosed.

If GitHub Security Advisories are not available to you, open a regular issue asking to be contacted privately, without describing the vulnerability itself.

When reporting, please include:

- a description of the vulnerability and its potential impact
- steps to reproduce it (a minimal `smidr.toml` and/or project layout, if relevant)
- the affected version or commit

## What counts as a security issue here

`smidr` executes external processes (the C compiler, `cmake`, `meson`, `make`, and - for `build_system = "custom"` - arbitrary shell commands defined in `smidr.toml`). Relevant reports include, for example:

- a way for a malformed `smidr.toml` or project layout to cause command or code execution the user did not intend
- path traversal or unintended file writes outside the project directory
- a way for `smidr new`, `smidr build`, or `smidr run` to behave unsafely on untrusted input

Note that `build_system = "custom"` is documented as executing arbitrary shell commands from `smidr.toml` by design - running a `smidr.toml` file from a source you don't trust is inherently equivalent to running its `build_commands` yourself. That is expected behavior, not a vulnerability, but reports on how to make this clearer or safer by default are welcome.

## Response

`smidr` is a personal project maintained alongside my studies, so response times may be slower than you'd expect from a full-time maintained project. Reports won't be ignored, but please allow some time - feel free to follow up if you haven't heard back after a couple of weeks.
