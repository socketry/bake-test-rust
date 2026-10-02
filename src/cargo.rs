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
mod tests {
    use super::{metadata_in, update_in};
    use std::fs;
    use std::path::PathBuf;
    use std::time::{SystemTime, UNIX_EPOCH};

    struct TemporaryDirectory(PathBuf);

    impl TemporaryDirectory {
        fn new() -> Self {
            let unique = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .expect("system time is after the Unix epoch")
                .as_nanos();
            let path = std::env::temp_dir().join(format!(
                "bake-test-rust-cargo-update-{}-{unique}",
                std::process::id()
            ));
            fs::create_dir_all(&path).expect("create temporary project");
            Self(path)
        }

        fn path(&self) -> &std::path::Path {
            &self.0
        }
    }

    impl Drop for TemporaryDirectory {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }

    #[test]
    fn updates_lockfile_to_select_a_new_local_patch_version() {
        let directory = TemporaryDirectory::new();
        let patched_package = directory.path().join("patched");
        fs::create_dir_all(patched_package.join("src")).expect("create patched package");
        fs::write(
            directory.path().join("Cargo.toml"),
            "[package]\nname = \"consumer\"\nversion = \"0.1.0\"\nedition = \"2024\"\n\n[dependencies]\npatched = \"0.1.0\"\n\n[patch.crates-io]\npatched = { path = \"patched\" }\n",
        )
        .expect("write consumer manifest");
        fs::create_dir_all(directory.path().join("src")).expect("create consumer source");
        fs::write(directory.path().join("src/main.rs"), "fn main() {}\n")
            .expect("write consumer source");
        fs::write(
            patched_package.join("Cargo.toml"),
            "[package]\nname = \"patched\"\nversion = \"0.1.0\"\nedition = \"2024\"\n",
        )
        .expect("write patched package manifest");
        fs::write(patched_package.join("src/lib.rs"), "pub fn value() {}\n")
            .expect("write patched package source");

        metadata_in(directory.path(), false).expect("resolve initial local package version");
        let patched_manifest = patched_package.join("Cargo.toml");
        fs::write(
            &patched_manifest,
            "[package]\nname = \"patched\"\nversion = \"0.1.1\"\nedition = \"2024\"\n",
        )
        .expect("bump patched package version");

        update_in(directory.path()).expect("update local patch version in the lockfile");

        let lockfile =
            fs::read_to_string(directory.path().join("Cargo.lock")).expect("read updated lockfile");
        assert!(lockfile.contains("name = \"patched\"\nversion = \"0.1.1\""));
    }
}
