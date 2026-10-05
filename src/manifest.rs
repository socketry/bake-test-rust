// Released under the MIT License.
// Copyright, 2026, by Samuel Williams.

use bake::{Error, Result};
use std::fs;
use std::io;
use std::path::{Component, Path, PathBuf};
use toml_edit::{DocumentMut, InlineTable, Item, Table, Value};

use crate::metadata::Package;

pub(crate) fn patch_checkout(checkout: &Path, packages: &[Package]) -> Result<()> {
    patch_checkout_with(
        checkout,
        packages,
        |path: &Path| fs::canonicalize(path),
        relative_path,
        |path, contents| fs::write(path, contents),
    )
}

fn patch_checkout_with(
    checkout: &Path,
    packages: &[Package],
    canonicalize: impl Fn(&Path) -> io::Result<PathBuf>,
    make_relative_path: impl Fn(&Path, &Path) -> Result<PathBuf>,
    write: impl Fn(&Path, &str) -> io::Result<()>,
) -> Result<()> {
    let manifest_path = checkout.join("Cargo.toml");
    let document = fs::read_to_string(&manifest_path)
        .map_err(|error| Error::new(format!("{}: {error}", manifest_path.display())))?;
    let mut manifest: DocumentMut = document.parse().map_err(|error| {
        Error::new(format!(
            "could not parse downstream manifest {}: {error}",
            manifest_path.display()
        ))
    })?;

    let checkout = canonicalize(checkout)?;
    let mut changed = false;

    for package in packages {
        let package_directory = package
            .manifest_path
            .parent()
            .ok_or_else(|| Error::new("workspace package manifest has no parent directory"))?;
        let package_directory = canonicalize(package_directory)?;
        let relative_path = make_relative_path(&checkout, &package_directory)?;
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
        write(&manifest_path, &manifest.to_string())?;
    }

    Ok(())
}

fn relative_path(from: &Path, to: &Path) -> Result<PathBuf> {
    let from_components: Vec<_> = from.components().collect();
    let to_components: Vec<_> = to.components().collect();
    relative_path_components(&from_components, &to_components)
}

fn relative_path_components(
    from_components: &[Component<'_>],
    to_components: &[Component<'_>],
) -> Result<PathBuf> {
    let common_length = from_components
        .iter()
        .zip(to_components.iter())
        .take_while(|(left, right)| left == right)
        .count();

    if common_length == 0 {
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
#[path = "manifest/tests.rs"]
mod tests;
