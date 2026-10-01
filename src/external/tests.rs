// Released under the MIT License.
// Copyright, 2026, by Samuel Williams.

use super::{checkout, ensure_local_patches_are_selected, normalize_repository, run_cargo};
use crate::metadata::{ExternalRepository, Package};
use crate::test_support::{Environment, TemporaryDirectory};
use bake::Registry;
use serde_json::json;
use std::fs;
use std::path::PathBuf;

fn repository(name: &str, url: &str) -> ExternalRepository {
    ExternalRepository {
        repository: url.to_owned(),
        branch: "main".to_owned(),
        name: name.to_owned(),
    }
}

fn package(identifier: &str, name: &str) -> Package {
    Package {
        package_identifier: identifier.to_owned(),
        name: name.to_owned(),
        version: "0.1.0".to_owned(),
        manifest_path: PathBuf::from("/workspace/local/Cargo.toml"),
    }
}

#[cfg(unix)]
fn fake_git_environment(
    temporary_directory: &TemporaryDirectory,
) -> (Environment, PathBuf, PathBuf) {
    let environment = Environment::new();
    let bin = temporary_directory.path().join("bin");
    fs::create_dir_all(&bin).expect("create fake command directory");
    super::super::test_support::install_fake_git(&bin);
    environment.prepend_path(&bin);
    let log = temporary_directory.path().join("git.log");
    fs::write(&log, "").expect("create fake Git log");
    environment.set("BAKE_TEST_GIT_LOG", &log);
    (environment, bin, log)
}

#[cfg(unix)]
#[test]
fn checks_out_repositories_and_validates_existing_origins() {
    let temporary_directory = TemporaryDirectory::new();
    let (_environment, _, log) = fake_git_environment(&temporary_directory);
    let context = Registry::new().context(temporary_directory.path());
    let repository = repository("downstream", "https://github.com/socketry/downstream.git");

    let missing_checkout = temporary_directory.path().join("external/downstream");
    fs::create_dir_all(&missing_checkout).expect("create non-checkout directory");
    let error = checkout(&context, &repository).expect_err("non-Git directory should fail");
    assert!(error.to_string().contains("is not a Git checkout"));

    fs::remove_dir_all(&missing_checkout).expect("remove non-checkout directory");
    let cloned = checkout(&context, &repository).expect("clone downstream repository");
    assert!(cloned.join(".git").is_dir());
    assert_eq!(
        fs::read_to_string(cloned.join("Cargo.toml")).unwrap(),
        "[workspace]\nmembers = []\n"
    );
    assert!(
        fs::read_to_string(&log)
            .unwrap()
            .contains("clone --branch=main -- https://github.com/socketry/downstream.git")
    );

    _environment.set(
        "BAKE_TEST_GIT_ORIGIN",
        "git@github.com:socketry/downstream.git",
    );
    checkout(&context, &repository).expect("reuse matching GitHub checkout");
    _environment.set(
        "BAKE_TEST_GIT_ORIGIN",
        "https://github.com/other/repository",
    );
    assert!(
        checkout(&context, &repository)
            .expect_err("different origin should fail")
            .to_string()
            .contains("is a checkout of")
    );
    _environment.set("BAKE_TEST_GIT_NO_ORIGIN", "true");
    assert!(
        checkout(&context, &repository)
            .expect_err("missing origin should fail")
            .to_string()
            .contains("could not read origin")
    );
}

#[cfg(unix)]
#[test]
fn reports_git_clone_failures() {
    let temporary_directory = TemporaryDirectory::new();
    let (environment, _, _) = fake_git_environment(&temporary_directory);
    let context = Registry::new().context(temporary_directory.path());
    environment.set("BAKE_TEST_GIT_FAIL", "true");

    assert!(
        checkout(&context, &repository("downstream", "owner/downstream"))
            .expect_err("failed clone should be reported")
            .to_string()
            .contains("git clone failed")
    );
}

#[test]
fn normalizes_github_repository_urls() {
    for (repository, expected) in [
        (
            "https://github.com/socketry/project.git/",
            "github.com/socketry/project",
        ),
        (
            "http://github.com/socketry/project.git",
            "github.com/socketry/project",
        ),
        (
            "ssh://git@github.com/socketry/project.git",
            "github.com/socketry/project",
        ),
        (
            "git@github.com:socketry/project.git",
            "github.com/socketry/project",
        ),
        (
            "https://example.com/project.git",
            "https://example.com/project",
        ),
    ] {
        assert_eq!(normalize_repository(repository), expected);
    }
}

#[cfg(unix)]
#[test]
fn validates_that_downstream_dependencies_select_local_packages() {
    let _environment = Environment::new();
    let temporary_directory = TemporaryDirectory::new();
    let cargo = crate::test_support::install_fake_cargo(temporary_directory.path());
    let log = temporary_directory.path().join("cargo.log");
    let metadata = temporary_directory.path().join("metadata.json");
    fs::write(&log, "").expect("create fake Cargo log");
    _environment.set("CARGO", cargo);
    _environment.set("BAKE_TEST_CARGO_LOG", &log);
    _environment.set("BAKE_TEST_DOWNSTREAM_METADATA", &metadata);

    let checkout = temporary_directory.path().join("downstream");
    fs::create_dir_all(&checkout).expect("create checkout directory");
    let local_package = package("path+file:///workspace/local#local@0.1.0", "local");
    fs::write(
        &metadata,
        json!({
            "packages": [{
                "id": "path+file:///workspace/local#local@0.1.0",
                "name": "local"
            }],
            "resolve": {"nodes": [{"deps": [
                {"pkg": "path+file:///workspace/local#local@0.1.0"},
                {"pkg": "unrelated-package-id"}
            ]}]}
        })
        .to_string(),
    )
    .expect("write selected local package metadata");
    ensure_local_patches_are_selected(&checkout, std::slice::from_ref(&local_package))
        .expect("accept a resolved local package");

    let error_documents = [
        (json!({}), "did not contain packages"),
        (json!({"packages": []}), "did not contain resolved nodes"),
        (
            json!({"packages": [], "resolve": {"nodes": [{"deps": [{"name": "local"}]}]}}),
            "did not contain a package identifier",
        ),
    ];
    for (document, expected) in error_documents {
        fs::write(&metadata, document.to_string()).expect("write invalid metadata");
        let error =
            ensure_local_patches_are_selected(&checkout, std::slice::from_ref(&local_package))
                .expect_err("invalid resolved dependency metadata should fail");
        assert!(error.to_string().contains(expected), "{error}");
    }

    fs::write(
        &metadata,
        json!({
            "packages": [{"id": "registry-id", "name": "local"}],
            "resolve": {"nodes": [{"deps": [{"pkg": "registry-id"}]}]}
        })
        .to_string(),
    )
    .expect("write registry package metadata");
    assert!(
        ensure_local_patches_are_selected(&checkout, std::slice::from_ref(&local_package))
            .expect_err("registry package must not satisfy the local patch")
            .to_string()
            .contains("did not resolve to this workspace's local package")
    );

    fs::write(&metadata, "not JSON").expect("write malformed metadata");
    assert!(
        ensure_local_patches_are_selected(&checkout, &[local_package])
            .expect_err("invalid JSON should fail")
            .to_string()
            .contains("could not parse downstream Cargo metadata")
    );
}

#[cfg(unix)]
#[test]
fn runs_downstream_cargo_commands_and_propagates_failures() {
    let _environment = Environment::new();
    let temporary_directory = TemporaryDirectory::new();
    let cargo = crate::test_support::install_fake_cargo(temporary_directory.path());
    let log = temporary_directory.path().join("cargo.log");
    fs::write(&log, "").expect("create fake Cargo log");
    _environment.set("CARGO", cargo);
    _environment.set("BAKE_TEST_CARGO_LOG", &log);

    let checkout = temporary_directory.path().join("downstream");
    fs::create_dir_all(&checkout).expect("create checkout directory");
    run_cargo(&checkout, &["test".to_owned()]).expect("run downstream tests");
    _environment.set("BAKE_TEST_FAIL_ON", "fail");
    assert!(
        run_cargo(&checkout, &["fail".to_owned()])
            .expect_err("downstream Cargo failure should propagate")
            .to_string()
            .contains("cargo fail failed in")
    );
}
