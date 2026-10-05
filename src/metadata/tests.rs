// Released under the MIT License.
// Copyright, 2026, by Samuel Williams.

use super::{
    parse_workspace, publishable_to_crates_io, read_external_repositories, read_workspace,
    required_string,
};
use crate::test_support::{Environment, TemporaryDirectory};
use bake::Registry;
use serde_json::Value;
use serde_json::json;
use std::fs;
use std::path::{Path, PathBuf};

#[test]
fn normalizes_and_sorts_external_repositories() {
    let metadata = json!({
        "metadata": {
            "bake": {
                "test": {
                    "external": [
                        {"repository": "socketry/zeta", "branch": "next"},
                        {"repository": "https://github.com/socketry/alpha.git"}
                    ]
                }
            }
        }
    });

    let repositories = read_external_repositories(&metadata, Path::new("."))
        .expect("valid external repository metadata");

    assert_eq!(repositories.len(), 2);
    assert_eq!(repositories[0].name, "alpha");
    assert_eq!(repositories[0].branch, "main");
    assert_eq!(
        repositories[0].repository,
        "https://github.com/socketry/alpha.git"
    );
    assert_eq!(repositories[1].name, "zeta");
    assert_eq!(repositories[1].branch, "next");
    assert_eq!(
        repositories[1].repository,
        "https://github.com/socketry/zeta"
    );
}

#[test]
fn rejects_checkout_names_that_escape_external_directory() {
    let metadata = json!({
        "metadata": {
            "bake": {
                "test": {
                    "external": [
                        {"repository": "socketry/example", "name": "../outside"}
                    ]
                }
            }
        }
    });

    assert!(read_external_repositories(&metadata, Path::new(".")).is_err());
}

fn package(id: &str, name: &str, publish: Option<Value>, manifest_path: &Path) -> Value {
    let mut package = json!({
        "id": id,
        "name": name,
        "version": "0.1.0",
        "manifest_path": manifest_path,
    });
    if let Some(publish) = publish {
        package["publish"] = publish;
    }
    package
}

fn valid_workspace(root: &Path) -> Value {
    json!({
        "workspace_root": root,
        "workspace_members": ["zeta-id", "alpha-id", "private-id", "restricted-id", "crates-io-id"],
        "packages": [
            package("zeta-id", "zeta", None, Path::new("/workspace/zeta/Cargo.toml")),
            package("alpha-id", "alpha", Some(Value::Bool(true)), Path::new("/workspace/alpha/Cargo.toml")),
            package("private-id", "private", Some(Value::Bool(false)), Path::new("/workspace/private/Cargo.toml")),
            package("restricted-id", "restricted", Some(json!(["other-registry"])), Path::new("/workspace/restricted/Cargo.toml")),
            package("tool-id", "tool", None, Path::new("/workspace/tool/Cargo.toml")),
            package("crates-io-id", "allowed", Some(json!(["other-registry", "crates-io"])), Path::new("/workspace/allowed/Cargo.toml")),
            package("invalid-publish-id", "invalid-publish", Some(json!({})), Path::new("/workspace/invalid/Cargo.toml"))
        ]
    })
}

#[test]
fn filters_and_sorts_workspace_packages() {
    let metadata = valid_workspace(Path::new("/workspace"));
    let workspace = parse_workspace(&metadata).expect("valid workspace metadata");

    assert_eq!(
        workspace
            .packages
            .iter()
            .map(|package| package.name.as_str())
            .collect::<Vec<_>>(),
        ["allowed", "alpha", "zeta"]
    );
    assert!(workspace.external_repositories.is_empty());
    assert_eq!(workspace.packages[0].version, "0.1.0");
    assert_eq!(
        workspace.packages[0].manifest_path,
        PathBuf::from("/workspace/allowed/Cargo.toml")
    );
}

#[test]
fn reads_workspace_metadata_through_cargo() {
    let _environment = Environment::new();
    let temporary_directory = TemporaryDirectory::new();
    let fake_cargo = crate::test_support::install_fake_cargo(temporary_directory.path());
    let log_path = temporary_directory.path().join("cargo.log");
    let metadata_path = temporary_directory.path().join("metadata.json");
    fs::write(&log_path, "").expect("create Cargo log");
    fs::write(
        &metadata_path,
        valid_workspace(temporary_directory.path()).to_string(),
    )
    .expect("write Cargo metadata fixture");
    _environment.set("CARGO", fake_cargo);
    _environment.set("BAKE_TEST_CARGO_LOG", &log_path);
    _environment.set("BAKE_TEST_WORKSPACE_METADATA", &metadata_path);

    let context = Registry::new().context(temporary_directory.path());
    let workspace = read_workspace(&context).expect("read workspace metadata");
    assert_eq!(workspace.packages.len(), 3);
    assert!(
        fs::read_to_string(log_path)
            .expect("read Cargo invocation")
            .contains("metadata --format-version 1 --no-deps")
    );

    fs::write(&metadata_path, "not JSON").expect("write invalid Cargo metadata");
    assert!(
        read_workspace(&context)
            .expect_err("invalid Cargo metadata should fail")
            .to_string()
            .contains("could not parse Cargo metadata")
    );

    _environment.set("BAKE_TEST_METADATA_EXIT", "7");
    assert!(
        read_workspace(&context)
            .expect_err("Cargo metadata command failures should propagate")
            .to_string()
            .contains("cargo metadata failed")
    );

    _environment.set("BAKE_TEST_METADATA_EXIT", "0");
    let invalid_external = json!({
        "workspace_root": temporary_directory.path(),
        "workspace_members": [],
        "packages": [],
        "metadata": {"bake": {"test": {"external": "invalid"}}}
    });
    fs::write(&metadata_path, invalid_external.to_string())
        .expect("write invalid external repository metadata");
    assert!(
        read_workspace(&context)
            .expect_err("external repository metadata should be validated")
            .to_string()
            .contains("must be an array")
    );
}

#[test]
fn rejects_invalid_workspace_metadata_shapes() {
    let root = "/workspace";
    let invalid_documents = [
        (json!({}), "workspace root"),
        (json!({"workspace_root": root}), "workspace members"),
        (
            json!({"workspace_root": root, "workspace_members": "bad"}),
            "workspace members",
        ),
        (
            json!({"workspace_root": root, "workspace_members": [1]}),
            "member was not a string",
        ),
        (
            json!({"workspace_root": root, "workspace_members": [], "packages": "bad"}),
            "did not contain packages",
        ),
    ];

    for (metadata, expected) in invalid_documents {
        let error = parse_workspace(&metadata).expect_err("invalid metadata should fail");
        assert!(error.to_string().contains(expected), "{error}");
    }

    for field in ["id", "name", "version", "manifest_path"] {
        let mut package = json!({
            "id": "package-id",
            "name": "package",
            "version": "0.1.0",
            "manifest_path": "/workspace/package/Cargo.toml"
        });
        package.as_object_mut().unwrap().remove(field);
        let metadata = json!({
            "workspace_root": "/workspace",
            "workspace_members": ["package-id"],
            "packages": [package]
        });
        let error = parse_workspace(&metadata).expect_err("missing package field should fail");
        assert!(error.to_string().contains(field), "{error}");
    }

    let duplicate_packages = json!({
        "workspace_root": "/workspace",
        "workspace_members": ["one", "two"],
        "packages": [
            package("one", "duplicate", None, Path::new("/workspace/one/Cargo.toml")),
            package("two", "duplicate", None, Path::new("/workspace/two/Cargo.toml"))
        ]
    });
    assert!(
        parse_workspace(&duplicate_packages)
            .expect_err("duplicate package names should fail")
            .to_string()
            .contains("more than one crates.io package")
    );
}

#[test]
fn publish_rules_accept_only_crates_io_publishable_packages() {
    assert!(publishable_to_crates_io(&json!({})));
    assert!(publishable_to_crates_io(&json!({"publish": null})));
    assert!(publishable_to_crates_io(&json!({"publish": true})));
    assert!(!publishable_to_crates_io(&json!({"publish": false})));
    assert!(publishable_to_crates_io(&json!({"publish": ["crates-io"]})));
    assert!(!publishable_to_crates_io(&json!({"publish": ["other", 1]})));
    assert!(!publishable_to_crates_io(&json!({"publish": "invalid"})));
}

#[test]
fn uses_root_package_metadata_when_workspace_metadata_is_absent() {
    let root = Path::new("/workspace");
    let metadata = json!({
        "packages": [{
            "manifest_path": "/workspace/Cargo.toml",
            "metadata": {"bake": {"test": {"external": [
                {"repository": "socketry/project"}
            ]}}}
        }]
    });

    let repositories =
        read_external_repositories(&metadata, root).expect("read root package external metadata");
    assert_eq!(repositories.len(), 1);
    assert_eq!(repositories[0].name, "project");

    let no_metadata = json!({"packages": []});
    assert!(
        read_external_repositories(&no_metadata, root)
            .expect("missing external metadata is empty")
            .is_empty()
    );

    let unrelated_root = json!({"packages": [{"manifest_path": "/other/Cargo.toml"}]});
    assert!(
        read_external_repositories(&unrelated_root, root)
            .expect("unrelated root package has no external metadata")
            .is_empty()
    );

    let missing_context = [
        json!({}),
        json!({"packages": "invalid"}),
        json!({"packages": [{"manifest_path": "/workspace/Cargo.toml"}]}),
        json!({"packages": [{"manifest_path": "/workspace/Cargo.toml", "metadata": {}}]}),
        json!({"packages": [{"manifest_path": "/workspace/Cargo.toml", "metadata": {"bake": {}}}]}),
        json!({"packages": [{"manifest_path": "/workspace/Cargo.toml", "metadata": {"bake": {"test": {}}}}]}),
    ];
    for metadata in missing_context {
        assert!(
            read_external_repositories(&metadata, root)
                .expect("missing root package context means no external tests")
                .is_empty()
        );
    }
}

#[test]
fn rejects_invalid_external_repository_metadata() {
    let invalid_documents = [
        (
            json!({"metadata": {"bake": {"test": {"external": "bad"}}}}),
            "must be an array",
        ),
        (
            json!({"metadata": {"bake": {"test": {"external": [{"branch": "main"}]}}}}),
            "repository",
        ),
        (
            json!({"metadata": {"bake": {"test": {"external": [{"repository": "owner/repo", "branch": 1}]}}}}),
            "branch must be a string",
        ),
        (
            json!({"metadata": {"bake": {"test": {"external": [{"repository": "owner/repo", "branch": ""}]}}}}),
            "branch must be nonempty",
        ),
        (
            json!({"metadata": {"bake": {"test": {"external": [{"repository": "owner/repo", "branch": "-main"}]}}}}),
            "cannot start with a hyphen",
        ),
        (
            json!({"metadata": {"bake": {"test": {"external": [{"repository": "owner/repo", "branch": "bad\nbranch"}]}}}}),
            "control characters",
        ),
        (
            json!({"metadata": {"bake": {"test": {"external": [{"repository": "owner/repo", "name": 1}]}}}}),
            "name must be a string",
        ),
        (
            json!({"metadata": {"bake": {"test": {"external": [{"repository": "owner/repo", "name": "bad/name"}]}}}}),
            "checkout names may contain only",
        ),
        (
            json!({"metadata": {"bake": {"test": {"external": [{"repository": "owner/repo", "name": ".."}]}}}}),
            "checkout names may contain only",
        ),
        (
            json!({"metadata": {"bake": {"test": {"external": [{"repository": "https://"}]}}}}),
            "could not derive a checkout name",
        ),
        (
            json!({"metadata": {"bake": {"test": {"external": [{"repository": "repository"}]}}}}),
            "Git URL or use owner/repository",
        ),
        (
            json!({"metadata": {"bake": {"test": {"external": [{"repository": "/repository"}]}}}}),
            "invalid owner/repository",
        ),
        (
            json!({"metadata": {"bake": {"test": {"external": [{"repository": "-owner/repository"}]}}}}),
            "invalid owner/repository",
        ),
        (
            json!({"metadata": {"bake": {"test": {"external": [{"repository": "owner/repo/extra"}]}}}}),
            "invalid owner/repository",
        ),
        (
            json!({"metadata": {"bake": {"test": {"external": [{"repository": "owner/.."}]}}}}),
            "checkout names may contain only",
        ),
        (
            json!({"metadata": {"bake": {"test": {"external": [{"repository": "owner/repo", "name": "same"}, {"repository": "other/repo", "name": "same"}]}}}}),
            "same checkout name",
        ),
    ];

    for (metadata, expected) in invalid_documents {
        let error = read_external_repositories(&metadata, Path::new("/workspace"))
            .expect_err("invalid external metadata should fail");
        assert!(error.to_string().contains(expected), "{error}");
    }
}

#[test]
fn preserves_supported_clone_urls_and_requires_nonempty_strings() {
    for repository in [
        "https://example.com/repo.git",
        "http://example.com/repo.git",
        "ssh://example.com/repo.git",
        "git://example.com/repo.git",
        "git@example.com:repo.git",
        "file:///tmp/repo.git",
    ] {
        let metadata = json!({
            "metadata": {"bake": {"test": {"external": [{"repository": repository}]}}}
        });
        let repositories = read_external_repositories(&metadata, Path::new("."))
            .expect("preserve a supported Git URL");
        assert_eq!(repositories[0].repository, repository);
    }

    for value in [json!({}), json!({"repository": ""})] {
        let metadata = json!({"metadata": {"bake": {"test": {"external": [value]}}}});
        assert!(
            read_external_repositories(&metadata, Path::new("."))
                .expect_err("missing repository should fail")
                .to_string()
                .contains("repository")
        );
    }

    assert!(required_string(&json!({"name": ""}), "name", "entry").is_err());
}
