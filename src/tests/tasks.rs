// Released under the MIT License.
// Copyright, 2026, by Samuel Williams.

use super::{run as run_task, run_coverage, run_external};
use crate::metadata::Package;
use crate::test_support::{Environment, TemporaryDirectory};
use bake::{Arguments, Context, Error, Registry, Result, Task, Value};
use serde_json::{Value as JsonValue, json};
use std::fs;
use std::path::Path;

fn hook_succeeds(context: &mut Context, _arguments: &Arguments) -> Result<Value> {
    fs::write(context.root().join("before-called"), "yes")?;
    Ok(Value::Null)
}

fn hook_fails(_context: &mut Context, _arguments: &Arguments) -> Result<Value> {
    Err(Error::new("pre-test hook failed"))
}

fn context(root: &Path) -> Context {
    Registry::new().context(root)
}

fn context_with_hook(root: &Path, hook: fn(&mut Context, &Arguments) -> Result<Value>) -> Context {
    let mut registry = Registry::new();
    registry
        .register(Task::new("test:before", "", Vec::new(), hook))
        .expect("register the test hook");
    registry.context(root)
}

#[cfg(unix)]
fn prepare_cargo(
    environment: &Environment,
    root: &Path,
    workspace_metadata: &JsonValue,
    downstream_metadata: &JsonValue,
) -> std::path::PathBuf {
    let cargo = crate::test_support::install_fake_cargo(root);
    let log = root.join("cargo.log");
    let workspace_metadata_path = root.join("workspace-metadata.json");
    let downstream_metadata_path = root.join("downstream-metadata.json");
    fs::write(&log, "").expect("create fake Cargo log");
    fs::write(&workspace_metadata_path, workspace_metadata.to_string())
        .expect("write workspace metadata fixture");
    fs::write(&downstream_metadata_path, downstream_metadata.to_string())
        .expect("write downstream metadata fixture");
    environment.set("CARGO", cargo);
    environment.set("BAKE_TEST_CARGO_LOG", &log);
    environment.set("BAKE_TEST_WORKSPACE_METADATA", &workspace_metadata_path);
    environment.set("BAKE_TEST_DOWNSTREAM_METADATA", &downstream_metadata_path);
    environment.set("BAKE_TEST_METADATA_EXIT", "0");
    environment.remove("BAKE_TEST_FAIL_ON");
    log
}

fn workspace_metadata(
    root: &Path,
    external: Vec<JsonValue>,
    packages: Vec<JsonValue>,
) -> JsonValue {
    json!({
        "workspace_root": root,
        "workspace_members": packages.iter().filter_map(|package| package["id"].as_str().map(str::to_owned)).collect::<Vec<_>>(),
        "packages": packages,
        "metadata": {"bake": {"test": {"external": external}}}
    })
}

fn local_package(root: &Path) -> (Package, JsonValue) {
    let manifest_path = root.join("local/Cargo.toml");
    fs::create_dir_all(manifest_path.parent().unwrap()).expect("create local package directory");
    fs::write(
        &manifest_path,
        "[package]\nname = \"local\"\nversion = \"0.1.0\"\n",
    )
    .expect("write local package manifest");

    let package = Package {
        package_identifier: "path+file:///workspace/local#local@0.1.0".to_owned(),
        name: "local".to_owned(),
        version: "0.1.0".to_owned(),
        manifest_path,
    };
    let metadata = json!({
        "id": package.package_identifier,
        "name": package.name,
        "version": package.version,
        "manifest_path": package.manifest_path,
    });
    (package, metadata)
}

fn downstream_metadata(package: &Package) -> JsonValue {
    json!({
        "packages": [{
            "id": package.package_identifier,
            "name": package.name,
        }],
        "resolve": {"nodes": [{"deps": [{"pkg": package.package_identifier}]}]}
    })
}

#[cfg(unix)]
#[test]
fn runs_workspace_tests_with_optional_targets_and_hook() {
    let temporary_directory = TemporaryDirectory::new();
    let environment = Environment::new();
    let log = prepare_cargo(
        &environment,
        temporary_directory.path(),
        &json!({}),
        &json!({}),
    );
    let mut context = context_with_hook(temporary_directory.path(), hook_succeeds);

    assert_eq!(
        run_task(&mut context, false).expect("run workspace tests"),
        "Cargo workspace tests passed"
    );
    assert!(temporary_directory.path().join("before-called").exists());
    let calls = fs::read_to_string(&log).expect("read fake Cargo calls");
    assert!(calls.contains("test --workspace --locked\n"));
    assert!(!calls.contains("--all-targets"));

    fs::write(&log, "").expect("clear fake Cargo log");
    assert_eq!(
        run_task(&mut context, true).expect("run all targets"),
        "Cargo workspace tests passed"
    );
    assert!(
        fs::read_to_string(log)
            .expect("read all-targets Cargo call")
            .contains("test --workspace --locked --all-targets")
    );
}

#[cfg(unix)]
#[test]
fn propagates_test_hook_and_cargo_failures() {
    let temporary_directory = TemporaryDirectory::new();
    let environment = Environment::new();
    let log = prepare_cargo(
        &environment,
        temporary_directory.path(),
        &json!({}),
        &json!({}),
    );
    let mut hook_context = context_with_hook(temporary_directory.path(), hook_fails);
    assert!(
        run_task(&mut hook_context, false)
            .expect_err("hook failure should stop tests")
            .to_string()
            .contains("pre-test hook failed")
    );
    assert!(fs::read_to_string(&log).expect("read Cargo log").is_empty());

    let mut context = context(temporary_directory.path());
    environment.set("BAKE_TEST_FAIL_ON", "test");
    assert!(
        run_task(&mut context, false)
            .expect_err("Cargo failure should propagate")
            .to_string()
            .contains("cargo test --workspace --locked failed")
    );
}

#[cfg(unix)]
#[test]
fn runs_coverage_with_package_feature_and_target_options() {
    let temporary_directory = TemporaryDirectory::new();
    let environment = Environment::new();
    let log = prepare_cargo(
        &environment,
        temporary_directory.path(),
        &json!({}),
        &json!({}),
    );
    let mut context = context(temporary_directory.path());

    assert_eq!(
        run_coverage(&mut context, false, vec![], None, false).expect("run default coverage task"),
        "Workspace tests passed with complete line coverage"
    );
    let calls = fs::read_to_string(&log).expect("read default coverage commands");
    assert!(calls.contains("test --workspace --locked --doc\n"));
    assert!(
        calls.contains(
            "llvm-cov --workspace --locked --fail-under-lines 100 --show-missing-lines\n"
        )
    );
    assert!(!calls.contains("--features"));

    fs::write(&log, "").expect("clear fake Cargo log");
    run_coverage(&mut context, true, vec![], Some("library".to_owned()), true)
        .expect("run all-features package coverage");
    let calls = fs::read_to_string(&log).expect("read all-features coverage commands");
    assert!(calls.contains("test --package library --locked --doc --all-features\n"));
    assert!(calls.contains("llvm-cov --package library --locked --fail-under-lines 100 --show-missing-lines --all-targets --all-features\n"));

    fs::write(&log, "").expect("clear fake Cargo log");
    run_coverage(
        &mut context,
        false,
        vec!["first".to_owned(), "second".to_owned()],
        None,
        false,
    )
    .expect("run selected feature coverage");
    let calls = fs::read_to_string(log).expect("read feature coverage commands");
    assert!(calls.contains("--features first,second"));
}

#[cfg(unix)]
#[test]
fn rejects_conflicting_coverage_options_and_reports_command_failures() {
    let temporary_directory = TemporaryDirectory::new();
    let environment = Environment::new();
    let log = prepare_cargo(
        &environment,
        temporary_directory.path(),
        &json!({}),
        &json!({}),
    );
    let mut context = context(temporary_directory.path());

    assert!(
        run_coverage(&mut context, true, vec!["feature".to_owned()], None, false)
            .expect_err("conflicting feature flags should fail")
            .to_string()
            .contains("choose either --all-features")
    );
    assert!(fs::read_to_string(&log).expect("read Cargo log").is_empty());

    environment.set("BAKE_TEST_FAIL_ON", "test");
    assert!(
        run_coverage(&mut context, false, vec![], None, false)
            .expect_err("documentation test failure should propagate")
            .to_string()
            .contains("cargo test --workspace --locked --doc failed")
    );

    environment.set("BAKE_TEST_FAIL_ON", "llvm-cov");
    assert!(
        run_coverage(&mut context, false, vec![], None, false)
            .expect_err("coverage command failure should propagate")
            .to_string()
            .contains("cargo llvm-cov --workspace")
    );
}

#[cfg(unix)]
#[test]
fn handles_empty_and_unpatchable_external_workspaces() {
    let temporary_directory = TemporaryDirectory::new();
    let environment = Environment::new();
    let log = prepare_cargo(
        &environment,
        temporary_directory.path(),
        &workspace_metadata(temporary_directory.path(), vec![], vec![]),
        &json!({}),
    );
    let mut context = context(temporary_directory.path());
    assert_eq!(
        run_external(&mut context, false).expect("no external tests configured"),
        "No downstream repositories are configured for external tests"
    );
    assert!(
        fs::read_to_string(&log)
            .expect("read metadata command")
            .contains("metadata --format-version 1 --no-deps")
    );

    let metadata = workspace_metadata(
        temporary_directory.path(),
        vec![json!({"repository": "socketry/downstream"})],
        vec![],
    );
    let metadata_path = temporary_directory.path().join("workspace-metadata.json");
    fs::write(metadata_path, metadata.to_string()).expect("write external workspace metadata");
    assert!(
        run_external(&mut context, false)
            .expect_err("external tests require local packages")
            .to_string()
            .contains("no crates.io packages")
    );
}

#[cfg(unix)]
#[test]
fn runs_external_tests_for_one_or_many_repositories() {
    for (repository_entries, expected_label, all_targets) in [
        (
            vec![json!({"repository": "socketry/one"})],
            "1 downstream repository",
            false,
        ),
        (
            vec![
                json!({"repository": "socketry/alpha"}),
                json!({"repository": "socketry/beta"}),
            ],
            "2 downstream repositories",
            true,
        ),
    ] {
        let temporary_directory = TemporaryDirectory::new();
        let environment = Environment::new();
        let (package, package_metadata) = local_package(temporary_directory.path());
        let metadata = workspace_metadata(
            temporary_directory.path(),
            repository_entries,
            vec![package_metadata],
        );
        let log = prepare_cargo(
            &environment,
            temporary_directory.path(),
            &metadata,
            &downstream_metadata(&package),
        );
        let bin = temporary_directory.path().join("bin");
        fs::create_dir_all(&bin).expect("create fake command directory");
        crate::test_support::install_fake_git(&bin);
        environment.prepend_path(&bin);
        let git_log = temporary_directory.path().join("git.log");
        fs::write(&git_log, "").expect("create fake Git log");
        environment.set("BAKE_TEST_GIT_LOG", &git_log);

        let mut context = context_with_hook(temporary_directory.path(), hook_succeeds);
        let result =
            run_external(&mut context, all_targets).expect("run configured downstream tests");
        assert!(result.contains(expected_label), "{result}");
        assert!(temporary_directory.path().join("before-called").exists());

        let cargo_calls = fs::read_to_string(log).expect("read Cargo test calls");
        let expected_count = if all_targets { 2 } else { 1 };
        assert_eq!(
            cargo_calls.matches("test --workspace").count(),
            expected_count
        );
        assert_eq!(cargo_calls.contains("--all-targets"), all_targets);

        let git_calls = fs::read_to_string(git_log).expect("read Git clone calls");
        assert_eq!(
            git_calls.matches("clone --branch=main").count(),
            expected_count
        );
        for name in ["one", "alpha", "beta"] {
            let manifest = temporary_directory
                .path()
                .join(format!("external/{name}/Cargo.toml"));
            if manifest.exists() {
                assert!(
                    fs::read_to_string(manifest)
                        .unwrap()
                        .contains("local = { path = \"../../local\" }")
                );
            }
        }
    }
}
