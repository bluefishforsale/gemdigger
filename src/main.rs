//! Placeholder binary so the template builds, tests, and releases end to end.
//! Replace `run` with the real program; the plumbing around it is the template.

use std::io::Write;

use anyhow::{Context, bail};

fn main() -> anyhow::Result<()> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    run(&args, &mut std::io::stdout().lock())
}

fn run(args: &[String], out: &mut impl Write) -> anyhow::Result<()> {
    let name = env!("CARGO_PKG_NAME");
    match args.first().map(String::as_str) {
        None => writeln!(out, "Hello from {name}"),
        // NOTE: CI and install-windows.ps1 read this line to confirm which build ran.
        Some("--version" | "-V") => writeln!(out, "{name} {}", env!("CARGO_PKG_VERSION")),
        Some(other) => bail!("unknown argument \"{other}\""),
    }
    .context("unable to write to stdout")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn output(args: &[&str]) -> anyhow::Result<String> {
        let args: Vec<String> = args.iter().map(|a| a.to_string()).collect();
        let mut out = Vec::new();
        run(&args, &mut out)?;
        Ok(String::from_utf8(out)?)
    }

    /// `--version` must report the Cargo.toml version, since release checks key off it.
    #[test]
    fn version_flag_reports_cargo_version() {
        let out = output(&["--version"]).unwrap();
        assert_eq!(
            out.trim(),
            format!("windows-exe-template {}", env!("CARGO_PKG_VERSION")),
            "unexpected version line: {out:?}"
        );
    }

    /// An unknown argument is an error, not silently ignored.
    #[test]
    fn unknown_argument_is_rejected() {
        let err = output(&["--bogus"]).unwrap_err();
        assert!(
            err.to_string().contains("--bogus"),
            "error should name the argument: {err}"
        );
    }
}
