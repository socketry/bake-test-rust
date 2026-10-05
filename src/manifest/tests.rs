// Released under the MIT License.
// Copyright, 2026, by Samuel Williams.

use super::{patch_checkout, patch_checkout_with, relative_path, relative_path_components};
use crate::metadata::Package;
use crate::test_support::TemporaryDirectory;
use bake::Error;
use std::fs;
use std::path::PathBuf;

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
    let checkout = temporary_directory.path().join("downstream");
    let package_directory = temporary_directory.path().join("workspace/example");
    fs::create_dir_all(&checkout).expect("create downstream checkout");
    fs::create_dir_all(&package_directory).expect("create local package directory");
    let manifest_path = checkout.join("Cargo.toml");
    fs::write(&manifest_path, "[workspace]\nmembers = []\n").expect("write downstream manifest");
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
    let checkout = temporary_directory.path().join("downstream");
    let package_directory = temporary_directory.path().join("workspace/example");
    fs::create_dir_all(&checkout).expect("create downstream checkout");
    fs::create_dir_all(&package_directory).expect("create local package directory");
    fs::write(
        checkout.join("Cargo.toml"),
        "[patch.crates-io]\nexample = { path = \"../other\" }\n",
    )
    .expect("write downstream manifest");

    assert!(patch_checkout(&checkout, &[package(package_directory.join("Cargo.toml"))]).is_err());
}

#[test]
fn reports_unreadable_and_invalid_downstream_manifests() {
    let temporary_directory = TemporaryDirectory::new();
    let checkout = temporary_directory.path().join("downstream");
    fs::create_dir_all(&checkout).expect("create downstream checkout");
    let package_directory = temporary_directory.path().join("workspace/example");
    fs::create_dir_all(&package_directory).expect("create local package directory");
    let packages = [package(package_directory.join("Cargo.toml"))];

    assert!(
        patch_checkout(&checkout, &packages)
            .expect_err("missing manifest should fail")
            .to_string()
            .contains("Cargo.toml")
    );

    let manifest_path = checkout.join("Cargo.toml");
    fs::write(&manifest_path, "not valid toml = [").expect("write invalid manifest");
    assert!(
        patch_checkout(&checkout, &packages)
            .expect_err("invalid manifest should fail")
            .to_string()
            .contains("could not parse downstream manifest")
    );
}

#[test]
fn rejects_package_manifest_paths_without_a_parent() {
    let temporary_directory = TemporaryDirectory::new();
    let checkout = temporary_directory.path().join("downstream");
    fs::create_dir_all(&checkout).expect("create downstream checkout");
    fs::write(checkout.join("Cargo.toml"), "[workspace]\nmembers = []\n")
        .expect("write downstream manifest");

    let error = patch_checkout(&checkout, &[package(PathBuf::new())])
        .expect_err("manifest path without parent should fail");
    assert!(
        error
            .to_string()
            .contains("workspace package manifest has no parent directory")
    );

    let missing_parent = temporary_directory.path().join("missing/Cargo.toml");
    assert!(patch_checkout(&checkout, &[package(missing_parent)]).is_err());
}

#[test]
fn propagates_canonicalization_relative_path_and_write_errors() {
    let temporary_directory = TemporaryDirectory::new();
    let checkout = temporary_directory.path().join("downstream");
    let package_directory = temporary_directory.path().join("workspace/example");
    fs::create_dir_all(&checkout).expect("create downstream checkout");
    fs::create_dir_all(&package_directory).expect("create local package directory");
    let manifest_path = checkout.join("Cargo.toml");
    fs::write(&manifest_path, "[workspace]\nmembers = []\n").expect("write downstream manifest");
    let package = package(package_directory.join("Cargo.toml"));

    let error = patch_checkout_with(
        &checkout,
        std::slice::from_ref(&package),
        |path| {
            if path == checkout {
                Err(std::io::Error::other("checkout canonicalization failed"))
            } else {
                fs::canonicalize(path)
            }
        },
        relative_path,
        |path, contents| fs::write(path, contents),
    )
    .expect_err("checkout canonicalization failure should propagate");
    assert!(
        error
            .to_string()
            .contains("checkout canonicalization failed")
    );

    let error = patch_checkout_with(
        &checkout,
        std::slice::from_ref(&package),
        |path: &std::path::Path| fs::canonicalize(path),
        |_, _| Err(Error::new("relative path calculation failed")),
        |path, contents| fs::write(path, contents),
    )
    .expect_err("relative path failure should propagate");
    assert!(
        error
            .to_string()
            .contains("relative path calculation failed")
    );

    let error = patch_checkout_with(
        &checkout,
        std::slice::from_ref(&package),
        |path: &std::path::Path| fs::canonicalize(path),
        relative_path,
        |_, _| {
            Err(std::io::Error::new(
                std::io::ErrorKind::PermissionDenied,
                "manifest write failed",
            ))
        },
    )
    .expect_err("manifest write failure should propagate");
    assert!(error.to_string().contains("manifest write failed"));
}

#[test]
fn rejects_invalid_patch_table_shapes() {
    let temporary_directory = TemporaryDirectory::new();
    let checkout = temporary_directory.path().join("downstream");
    let package_directory = temporary_directory.path().join("workspace/example");
    fs::create_dir_all(&checkout).expect("create downstream checkout");
    fs::create_dir_all(&package_directory).expect("create local package directory");
    let local_package = package(package_directory.join("Cargo.toml"));

    fs::write(checkout.join("Cargo.toml"), "patch = 'invalid'\n")
        .expect("write invalid patch section");
    assert!(
        patch_checkout(&checkout, std::slice::from_ref(&local_package))
            .expect_err("non-table patch section should fail")
            .to_string()
            .contains("[patch] value must be a table")
    );

    fs::write(
        checkout.join("Cargo.toml"),
        "[patch]\ncrates-io = 'invalid'\n",
    )
    .expect("write invalid crates.io patch section");
    assert!(
        patch_checkout(&checkout, &[local_package])
            .expect_err("non-table crates.io patch section should fail")
            .to_string()
            .contains("[patch.crates-io] value must be a table")
    );
}

#[test]
fn preserves_matching_table_patches_and_empty_package_lists() {
    let temporary_directory = TemporaryDirectory::new();
    let checkout = temporary_directory.path().join("downstream");
    let package_directory = temporary_directory.path().join("workspace/example");
    fs::create_dir_all(&checkout).expect("create downstream checkout");
    fs::create_dir_all(&package_directory).expect("create local package directory");
    let manifest_path = checkout.join("Cargo.toml");
    let original = "[patch.crates-io.example]\npath = \"../workspace/example\"\n";
    fs::write(&manifest_path, original).expect("write matching table patch");
    let local_package = package(package_directory.join("Cargo.toml"));

    patch_checkout(&checkout, std::slice::from_ref(&local_package))
        .expect("accept an existing matching table patch");
    assert_eq!(fs::read_to_string(&manifest_path).unwrap(), original);

    patch_checkout(&checkout, &[]).expect("accept an empty package list");
    assert_eq!(fs::read_to_string(manifest_path).unwrap(), original);
}

#[test]
fn rejects_paths_without_a_shared_component_root() {
    let from: Vec<_> = std::path::Path::new("relative").components().collect();
    let to: Vec<_> = std::path::Path::new("/absolute").components().collect();

    assert!(
        relative_path_components(&from, &to)
            .expect_err("different roots should fail")
            .to_string()
            .contains("different filesystem roots")
    );
}
