// Released under the MIT License.
// Copyright, 2026, by Samuel Williams.

use super::{executable_from, metadata_in, run, run_in, update_in};
use crate::test_support::{Environment, TemporaryDirectory};
use bake::Registry;
use std::ffi::OsString;
use std::fs;

#[test]
fn executable_defaults_to_cargo() {
    assert_eq!(executable_from(None), OsString::from("cargo"));
}

#[test]
fn executable_uses_cargo_environment_value() {
    assert_eq!(
        executable_from(Some(OsString::from("custom-cargo"))),
        OsString::from("custom-cargo")
    );
}

#[cfg(unix)]
#[test]
fn cargo_commands_report_success_failure_and_startup_errors() {
    let _environment = Environment::new();
    let temporary_directory = TemporaryDirectory::new();
    let executable = crate::test_support::install_fake_cargo(temporary_directory.path());
    let log = temporary_directory.path().join("cargo.log");
    let context = Registry::new().context(temporary_directory.path());

    _environment.set("CARGO", &executable);
    _environment.set("BAKE_TEST_CARGO_LOG", &log);
    fs::write(&log, "").expect("create cargo log");

    run(&context, &["test".to_owned()]).expect("successful Cargo command");
    _environment.set("BAKE_TEST_FAIL_ON", "fail");
    let error = run(&context, &["fail".to_owned()]).expect_err("failed Cargo command");
    assert!(
        error
            .to_string()
            .contains("cargo fail failed: exit status: 7")
    );

    run_in(temporary_directory.path(), &["test".to_owned()])
        .expect("successful Cargo command in a checkout");
    let error = run_in(temporary_directory.path(), &["fail".to_owned()])
        .expect_err("failed Cargo command in a checkout");
    assert!(error.to_string().contains("cargo fail failed in "));
    _environment.remove("BAKE_TEST_FAIL_ON");

    let metadata = temporary_directory.path().join("metadata.json");
    fs::write(&metadata, "{\"packages\":[]}").expect("write fake metadata");
    _environment.set("BAKE_TEST_WORKSPACE_METADATA", &metadata);
    let bytes =
        metadata_in(temporary_directory.path(), true).expect("read successful Cargo metadata");
    assert_eq!(bytes, b"{\"packages\":[]}");

    _environment.set("BAKE_TEST_METADATA_EXIT", "7");
    let error =
        metadata_in(temporary_directory.path(), false).expect_err("failed Cargo metadata command");
    assert!(error.to_string().contains("cargo metadata failed in "));

    _environment.set("CARGO", temporary_directory.path().join("missing-cargo"));
    assert!(run(&context, &["test".to_owned()]).is_err());
    assert!(run_in(temporary_directory.path(), &["test".to_owned()]).is_err());
    assert!(metadata_in(temporary_directory.path(), false).is_err());

    let log = fs::read_to_string(log).expect("read recorded Cargo commands");
    assert!(log.contains("metadata --format-version 1 --no-deps"));
    assert!(log.contains("metadata --format-version 1\n"));
}

#[test]
fn updates_lockfile_to_select_a_new_local_patch_version() {
    let _environment = Environment::new();
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
    fs::write(
        patched_package.join("Cargo.toml"),
        "[package]\nname = \"patched\"\nversion = \"0.1.1\"\nedition = \"2024\"\n",
    )
    .expect("bump patched package version");

    update_in(directory.path()).expect("update local patch version in the lockfile");

    let lockfile =
        fs::read_to_string(directory.path().join("Cargo.lock")).expect("read updated lockfile");
    assert!(lockfile.contains("name = \"patched\"\nversion = \"0.1.1\""));
}
