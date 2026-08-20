// SPDX-License-Identifier: MIT OR Apache-2.0
// Copyright (c) 2026 Max-Mend
// This file is part of smidr: https://github.com/Max-Mend/smidr

//! Pretty-printing for compiler diagnostics, in the same visual style as
//! `rustc`/`cargo`: colored severity, a `-->` location line, the
//! offending source line, and a caret pointing at the column.
//!
//! Parses gcc/clang's default single-line diagnostic format:
//! `file:line:column: severity: message`.

use std::fmt;

const RESET: &str = "\x1b[0m";
const BOLD: &str = "\x1b[1m";
const BLUE: &str = "\x1b[1;94m";

/// Severity of a single diagnostic, drives both its color and how it's
/// counted in [`print_summary`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Severity {
    Error,
    Warning,
    Note,
}

impl Severity {
    /// Parse the severity word gcc/clang put between the second and
    /// third colon (`error`, `warning`, `note`), including gcc's
    /// two-word `fatal error`.
    fn parse(s: &str) -> Option<Self> {
        match s.trim() {
            "error" | "fatal error" => Some(Self::Error),
            "warning" => Some(Self::Warning),
            "note" => Some(Self::Note),
            _ => None,
        }
    }

    fn color(self) -> &'static str {
        match self {
            Self::Error => "\x1b[1;91m",
            Self::Warning => "\x1b[1;93m",
            Self::Note => "\x1b[1;96m",
        }
    }

    fn label(self) -> &'static str {
        match self {
            Self::Error => "error",
            Self::Warning => "warning",
            Self::Note => "note",
        }
    }
}

impl fmt::Display for Severity {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.label())
    }
}

/// A single diagnostic, parsed from one line of gcc/clang stderr output.
#[derive(Debug, Clone)]
pub struct Diagnostic {
    pub file: String,
    pub line: usize,
    pub column: usize,
    pub severity: Severity,
    pub message: String,
}

impl Diagnostic {
    /// Parse one line of gcc/clang diagnostic output
    /// (`file:line:column: severity: message`).
    ///
    /// Returns `None` for anything that doesn't match - continuation
    /// lines like "In file included from ...", blank lines, or source
    /// snippets the compiler echoes back - so callers can filter a raw
    /// stderr blob with `.filter_map(Diagnostic::parse_line)` and simply
    /// ignore what isn't a diagnostic, rather than erroring on it.
    pub fn parse_line(line: &str) -> Option<Self> {
        let mut parts = line.splitn(4, ':');
        let file = parts.next()?.trim();
        let line_num: usize = parts.next()?.trim().parse().ok()?;
        let col_num: usize = parts.next()?.trim().parse().ok()?;
        let rest = parts.next()?.trim();

        let (sev_str, message) = rest.split_once(':')?;
        let severity = Severity::parse(sev_str)?;

        if file.is_empty() || line_num == 0 || col_num == 0 {
            return None;
        }

        Some(Self {
            file: file.to_string(),
            line: line_num,
            column: col_num,
            severity,
            message: message.trim().to_string(),
        })
    }
}

/// Parse every diagnostic out of a raw compiler stderr blob, silently
/// skipping lines that aren't diagnostics.
pub fn parse_all(stderr: &str) -> Vec<Diagnostic> {
    stderr.lines().filter_map(Diagnostic::parse_line).collect()
}

/// Pretty-print a single diagnostic: colored severity and message, a
/// `-->` location line, the source line itself, and a caret aligned to
/// the reported column (tabs in the source are preserved as tabs in the
/// padding, so the caret lines up visually regardless of indentation
/// style).
pub fn print_diagnostic(diag: &Diagnostic) {
    let color = diag.severity.color();
    let gutter = diag.line.to_string().len().max(2);

    eprintln!(
        "{color}{BOLD}{}{RESET}{BOLD}: {}{RESET}",
        diag.severity, diag.message
    );
    eprintln!(
        "{BLUE}{:>gutter$}{RESET} {BLUE}-->{RESET} {}:{}:{}",
        "",
        diag.file,
        diag.line,
        diag.column,
        gutter = gutter
    );
    eprintln!("{BLUE}{:>gutter$} |{RESET}", "", gutter = gutter);

    if let Ok(content) = std::fs::read_to_string(&diag.file) {
        if let Some(code_line) = content.lines().nth(diag.line.saturating_sub(1)) {
            eprintln!(
                "{BLUE}{:>gutter$} |{RESET} {}",
                diag.line,
                code_line,
                gutter = gutter
            );

            let caret_pad = diag.column.saturating_sub(1);
            let padding: String = code_line
                .chars()
                .take(caret_pad)
                .map(|c| if c == '\t' { '\t' } else { ' ' })
                .collect();
            eprintln!(
                "{BLUE}{:>gutter$} |{RESET} {}{color}{BOLD}^{RESET}",
                "",
                padding,
                gutter = gutter
            );
        }
    }
    eprintln!("{BLUE}{:>gutter$} |{RESET}", "", gutter = gutter);
    eprintln!();
}

/// Print all diagnostics, then a `cargo`-style summary line
/// (`error: could not compile due to 3 previous errors`).
pub fn print_all(diagnostics: &[Diagnostic]) {
    for diag in diagnostics {
        print_diagnostic(diag);
    }
    print_summary(diagnostics);
}

/// Print the final summary line after all diagnostics, counting errors
/// and warnings separately.
pub fn print_summary(diagnostics: &[Diagnostic]) {
    let errors = diagnostics
        .iter()
        .filter(|d| d.severity == Severity::Error)
        .count();
    let warnings = diagnostics
        .iter()
        .filter(|d| d.severity == Severity::Warning)
        .count();

    if errors > 0 {
        let noun = if errors == 1 { "error" } else { "errors" };
        eprintln!(
            "{}{}error{}: could not compile due to {} previous {}",
            BOLD,
            Severity::Error.color(),
            RESET,
            errors,
            noun
        );
    } else if warnings > 0 {
        let noun = if warnings == 1 { "warning" } else { "warnings" };
        eprintln!(
            "{}{}warning{}: {} {} emitted",
            BOLD,
            Severity::Warning.color(),
            RESET,
            warnings,
            noun
        );
    }
}
