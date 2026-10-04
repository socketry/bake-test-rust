// Released under the MIT License.
// Copyright, 2026, by Samuel Williams.

use bake::{Context, Error, Result};
use std::ffi::OsString;
use std::path::Path;
use std::process::Command;

pub(crate) fn executable() -> OsString {
    executable_from(std::env::var_os("CARGO"))
}

fn executable_from(cargo: Option<OsString>) -> OsString {
    cargo.unwrap_or_else(|| "cargo".into())
}

pub(crate) fn run(context: &Context, arguments: &[String]) -> Result<()> {
    let status = context.command(executable()).args(arguments).status()?;
    if status.success() {
        Ok(())
    } else {
        Err(Error::new(format!(
            "cargo {} failed: {status}",
            arguments.join(" ")
        )))
    }
}

pub(crate) fn run_with_output_path(
    context: &Context,
    arguments: &[String],
    output_path: &Path,
) -> Result<()> {
    let status = context
        .command(executable())
        .args(arguments)
        .args(["--json", "--show-missing-lines", "--output-path"])
        .arg(output_path)
        .status()?;
    if status.success() {
        Ok(())
    } else {
        Err(Error::new(format!(
            "cargo {} --json --show-missing-lines --output-path {} failed: {status}",
            arguments.join(" "),
            output_path.display()
        )))
    }
}

pub(crate) fn run_in(directory: &std::path::Path, arguments: &[String]) -> Result<()> {
    let status = Command::new(executable())
        .current_dir(directory)
        .args(arguments)
        .status()?;
    if status.success() {
        Ok(())
    } else {
        Err(Error::new(format!(
            "cargo {} failed in {}: {status}",
            arguments.join(" "),
            directory.display()
        )))
    }
}

pub(crate) fn update_in(directory: &std::path::Path) -> Result<()> {
    run_in(directory, &["update".to_owned()])
}

pub(crate) fn metadata_in(directory: &std::path::Path, no_dependencies: bool) -> Result<Vec<u8>> {
    let mut command = Command::new(executable());
    command
        .current_dir(directory)
        .args(["metadata", "--format-version", "1"]);
    if no_dependencies {
        command.arg("--no-deps");
    }

    let output = command.output()?;
    if output.status.success() {
        Ok(output.stdout)
    } else {
        Err(Error::new(format!(
            "cargo metadata failed in {}: {}",
            directory.display(),
            String::from_utf8_lossy(&output.stderr).trim()
        )))
    }
}

#[cfg(test)]
#[path = "cargo/tests.rs"]
mod tests;
