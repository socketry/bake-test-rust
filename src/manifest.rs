// Released under the MIT License.
// Copyright, 2026, by Samuel Williams.

use bake::{Error, Result};
use std::fs;
use std::path::{Component, Path, PathBuf};
use toml_edit::{DocumentMut, InlineTable, Item, Table, Value};

use crate::metadata::Package;

pub(crate) fn patch_checkout(checkout: &Path, packages: &[Package]) -> Result<()> {
    let manifest_path = checkout.join("Cargo.toml");
    let document = fs::read_to_string(&manifest_path)
        .map_err(|error| Error::new(format!("{}: {error}", manifest_path.display())))?;
    let mut manifest: DocumentMut = document.parse().map_err(|error| {
        Error::new(format!(
            "could not parse downstream manifest {}: {error}",
            manifest_path.display()
        ))
    })?;

    let checkout = checkout.canonicalize()?;
    let mut changed = false;

    for package in packages {
        let package_directory = package
            .manifest_path
            .parent()
            .ok_or_else(|| Error::new("workspace package manifest has no parent directory"))?
            .canonicalize()?;
        let relative_path = relative_path(&checkout, &package_directory)?;
        let relative_path = relative_path.to_string_lossy().replace('\\', "/");

        let patch = manifest
            .as_table_mut()
            .entry("patch")
            .or_insert_with(|| Item::Table(Table::new()));
        let patch_table = patch
            .as_table_mut()
            .ok_or_else(|| Error::new("downstream Cargo.toml [patch] value must be a table"))?;
        let crates_io = patch_table
            .entry("crates-io")
            .or_insert_with(|| Item::Table(Table::new()));
        let crates_io = crates_io.as_table_mut().ok_or_else(|| {
            Error::new("downstream Cargo.toml [patch.crates-io] value must be a table")
        })?;

        if let Some(existing) = crates_io.get(&package.name) {
            let existing_path = existing
                .as_inline_table()
                .and_then(|table| table.get("path"))
                .and_then(Value::as_str)
                .or_else(|| {
                    existing
                        .as_table()
                        .and_then(|table| table.get("path"))
                        .and_then(Item::as_str)
                });
            if existing_path == Some(relative_path.as_str()) {
                continue;
            }
            return Err(Error::new(format!(
                "downstream Cargo.toml already patches {:?} in [patch.crates-io]; resolve that entry manually before running external tests",
                package.name
            )));
        }

        let mut local_package = InlineTable::new();
        local_package.insert("path", Value::from(relative_path));
        crates_io.insert(
            &package.name,
            Item::Value(Value::InlineTable(local_package)),
        );
        changed = true;
    }

    if changed {
        fs::write(manifest_path, manifest.to_string())?;
    }

    Ok(())
}

fn relative_path(from: &Path, to: &Path) -> Result<PathBuf> {
    let from_components: Vec<_> = from.components().collect();
    let to_components: Vec<_> = to.components().collect();
    let common_length = from_components
        .iter()
        .zip(&to_components)
        .take_while(|(left, right)| left == right)
        .count();

    if common_length == 0
        || matches!(from_components.first(), Some(Component::Prefix(_)))
            && !matches!(to_components.first(), Some(Component::Prefix(_)))
    {
        return Err(Error::new(
            "cannot make a relative Cargo patch path between different filesystem roots",
        ));
    }

    let mut result = PathBuf::new();
    for _ in common_length..from_components.len() {
        result.push("..");
    }
    for component in &to_components[common_length..] {
        result.push(component.as_os_str());
    }

    Ok(result)
}

#[cfg(test)]
mod tests {
    use super::patch_checkout;
    use crate::metadata::Package;
    use std::fs;
    use std::path::PathBuf;
    use std::time::{SystemTime, UNIX_EPOCH};

    struct TemporaryDirectory(PathBuf);

    impl TemporaryDirectory {
        fn new() -> Self {
            let unique = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .expect("system clock is after the Unix epoch")
                .as_nanos();
            let path = std::env::temp_dir()
                .join(format!("bake-test-rust-{}-{unique}", std::process::id()));
            fs::create_dir_all(&path).expect("create temporary test directory");
            Self(path)
        }
    }

    impl Drop for TemporaryDirectory {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }

    fn package(manifest_path: PathBuf) -> Package {
        Package {
            package_identifier: "path+file:///workspace/example#example@0.1.0".to_owned(),
            name: "example".to_owned(),
            version: "0.1.0".to_owned(),
            manifest_path,
        }
    }

    #[test]
    fn adds_a_relative_patch_and_is_idempotent() {
        let temporary_directory = TemporaryDirectory::new();
        let checkout = temporary_directory.0.join("downstream");
        let package_directory = temporary_directory.0.join("workspace/example");
        fs::create_dir_all(&checkout).expect("create downstream checkout");
        fs::create_dir_all(&package_directory).expect("create local package directory");
        let manifest_path = checkout.join("Cargo.toml");
        fs::write(&manifest_path, "[workspace]\nmembers = []\n")
            .expect("write downstream manifest");
        let local_package = package(package_directory.join("Cargo.toml"));

        patch_checkout(&checkout, std::slice::from_ref(&local_package))
            .expect("add a local package patch");
        let patched_manifest = fs::read_to_string(&manifest_path).expect("read patched manifest");
        let parsed_manifest: toml_edit::DocumentMut =
            patched_manifest.parse().expect("parse patched manifest");
        assert_eq!(
            parsed_manifest["patch"]["crates-io"]["example"]["path"].as_str(),
            Some("../workspace/example")
        );

        patch_checkout(&checkout, &[local_package]).expect("reapply existing local patch");
        assert_eq!(
            fs::read_to_string(manifest_path).expect("read idempotent manifest"),
            patched_manifest
        );
    }

    #[test]
    fn rejects_a_conflicting_existing_patch() {
        let temporary_directory = TemporaryDirectory::new();
        let checkout = temporary_directory.0.join("downstream");
        let package_directory = temporary_directory.0.join("workspace/example");
        fs::create_dir_all(&checkout).expect("create downstream checkout");
        fs::create_dir_all(&package_directory).expect("create local package directory");
        fs::write(
            checkout.join("Cargo.toml"),
            "[patch.crates-io]\nexample = { path = \"../other\" }\n",
        )
        .expect("write downstream manifest");

        assert!(
            patch_checkout(&checkout, &[package(package_directory.join("Cargo.toml"))]).is_err()
        );
    }
}
