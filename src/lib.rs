// Released under the MIT License.
// Copyright, 2026, by Samuel Williams.

//! Shared test tasks for Rust projects using Bake.
//!
//! The crate provides `test` and `test:external`. Both tasks call the optional
//! project task `test:before` before running Cargo tests.
mod cargo;
mod external;
mod manifest;
mod metadata;

use bake::{Context, Error, Result};

fn run_before_test_hook(context: &mut Context) -> Result<()> {
    context.call_if_registered("test:before", &[])?;
    Ok(())
}

/// Run the current Cargo workspace tests with the lockfile enforced.
#[bake::task(name = "test")]
fn run(
    context: &mut Context,
    #[bake(named, default = false, help = "Also test examples and benchmarks.")] all_targets: bool,
) -> Result<String> {
    run_before_test_hook(context)?;

    let mut arguments = vec![
        "test".to_owned(),
        "--workspace".to_owned(),
        "--locked".to_owned(),
    ];
    if all_targets {
        arguments.push("--all-targets".to_owned());
    }

    cargo::run(context, &arguments)?;
    Ok("Cargo workspace tests passed".to_owned())
}

/// Test configured downstream repositories against this workspace's local packages.
#[bake::task(name = "test:external")]
fn run_external(
    context: &mut Context,
    #[bake(named, default = false, help = "Also test examples and benchmarks.")] all_targets: bool,
) -> Result<String> {
    let workspace = metadata::read_workspace(context)?;
    if workspace.external_repositories.is_empty() {
        return Ok("No downstream repositories are configured for external tests".to_owned());
    }
    if workspace.packages.is_empty() {
        return Err(Error::new(
            "the current workspace has no crates.io packages to patch into downstream tests",
        ));
    }

    let mut checkouts = Vec::with_capacity(workspace.external_repositories.len());
    for repository in &workspace.external_repositories {
        let checkout = external::checkout(context, repository)?;
        manifest::patch_checkout(&checkout, &workspace.packages)?;
        cargo::update_in(&checkout)?;
        external::ensure_local_patches_are_selected(&checkout, &workspace.packages)?;
        checkouts.push(checkout);
    }

    run_before_test_hook(context)?;

    for checkout in &checkouts {
        let mut arguments = vec!["test".to_owned(), "--workspace".to_owned()];
        if all_targets {
            arguments.push("--all-targets".to_owned());
        }
        external::run_cargo(checkout, &arguments)?;
    }

    let repository_count = checkouts.len();
    let repository_label = if repository_count == 1 {
        "repository"
    } else {
        "repositories"
    };
    Ok(format!(
        "External tests passed for {repository_count} downstream {repository_label}"
    ))
}
