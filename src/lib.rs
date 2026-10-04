// Released under the MIT License.
// Copyright, 2026, by Samuel Williams.

//! Shared test tasks for Rust projects using Bake.
//!
//! The crate provides `test`, `test:coverage`, and `test:external`. Each task
//! calls the optional project task `test:before` before running Cargo tests.
mod cargo;
mod coverage;
mod external;
mod manifest;
mod metadata;
#[cfg(test)]
#[path = "tests/support.rs"]
mod test_support;

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

/// Run workspace tests and require complete line coverage.
///
/// Documentation tests run through Cargo because `cargo-llvm-cov` does not
/// currently support stable doctest coverage. The remaining test targets run
/// under `cargo-llvm-cov`; uncovered lines fail the task unless they are
/// explicitly marked as unreachable.
#[bake::task(name = "test:coverage")]
fn run_coverage(
    context: &mut Context,
    #[bake(named, default = false, help = "Test every feature together.")] all_features: bool,
    #[bake(help = "Repeat to select features.")] features: Vec<String>,
    #[bake(help = "Limit coverage to one workspace package.")] package: Option<String>,
    #[bake(named, default = false, help = "Also include examples and benchmarks.")]
    all_targets: bool,
) -> Result<String> {
    if all_features && !features.is_empty() {
        return Err(Error::new(
            "choose either --all-features true or one or more --features values",
        ));
    }

    run_before_test_hook(context)?;

    let mut doc_arguments = vec!["test".to_owned()];
    append_package_arguments(&mut doc_arguments, package.as_deref());
    doc_arguments.extend(["--locked".to_owned(), "--doc".to_owned()]);
    append_feature_arguments(&mut doc_arguments, all_features, &features);
    cargo::run(context, &doc_arguments)?;

    let report = coverage::CoverageReport::new()?;
    let mut coverage_arguments = vec!["llvm-cov".to_owned()];
    append_package_arguments(&mut coverage_arguments, package.as_deref());
    coverage_arguments.push("--locked".to_owned());
    if all_targets {
        coverage_arguments.push("--all-targets".to_owned());
    }
    append_feature_arguments(&mut coverage_arguments, all_features, &features);
    cargo::run_with_output_path(context, &coverage_arguments, report.path())?;

    let summary = report.read(context.root())?;
    if !summary.is_complete() {
        return Err(Error::new(summary.failure_message()));
    }

    Ok(summary.success_message())
}

fn append_feature_arguments(arguments: &mut Vec<String>, all_features: bool, features: &[String]) {
    if all_features {
        arguments.push("--all-features".to_owned());
    } else if !features.is_empty() {
        arguments.push("--features".to_owned());
        arguments.push(features.join(","));
    }
}

fn append_package_arguments(arguments: &mut Vec<String>, package: Option<&str>) {
    if let Some(package) = package {
        arguments.push("--package".to_owned());
        arguments.push(package.to_owned());
    } else {
        arguments.push("--workspace".to_owned());
    }
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

#[cfg(test)]
#[path = "tests/tasks.rs"]
mod tests;
