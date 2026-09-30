// Released under the MIT License.
// Copyright, 2026, by Samuel Williams.

use bake::{Context, Error, Result};
use std::ffi::OsString;
use std::process::Command;

pub(crate) fn executable() -> OsString {
    std::env::var_os("CARGO").unwrap_or_else(|| "cargo".into())
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
