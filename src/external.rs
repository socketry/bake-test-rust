// Released under the MIT License.
// Copyright, 2026, by Samuel Williams.

use bake::{Error, Result};
use serde_json::Value;
use std::path::{Path, PathBuf};
use std::process::Command;

use crate::cargo;
use crate::metadata::{ExternalRepository, Package};

pub(crate) fn checkout(
    context: &bake::Context,
    repository: &ExternalRepository,
) -> Result<PathBuf> {
    let external_root = context.root().join("external");
    let destination = external_root.join(&repository.name);
    if destination.exists() {
        if !destination.join(".git").exists() {
            return Err(Error::new(format!(
                "{} exists but is not a Git checkout; move it before running external tests",
                destination.display()
            )));
        }
        ensure_origin(&destination, &repository.repository)?;
        return Ok(destination);
    }

    std::fs::create_dir_all(&external_root)?;
    let status = Command::new("git")
        .arg("clone")
        .arg(format!("--branch={}", repository.branch))
        .arg("--")
        .arg(&repository.repository)
        .arg(&destination)
        .status()?;
    if !status.success() {
        return Err(Error::new(format!(
            "git clone failed for {} at branch {:?}: {status}",
            repository.repository, repository.branch
        )));
    }

    Ok(destination)
}

fn ensure_origin(checkout: &Path, expected_repository: &str) -> Result<()> {
    let output = Command::new("git")
        .arg("-C")
        .arg(checkout)
        .args(["remote", "get-url", "origin"])
        .output()?;
    if !output.status.success() {
        return Err(Error::new(format!(
            "could not read origin for {}: {}",
            checkout.display(),
            String::from_utf8_lossy(&output.stderr).trim()
        )));
    }

    let actual_repository = String::from_utf8_lossy(&output.stdout).trim().to_owned();
    if normalize_repository(&actual_repository) != normalize_repository(expected_repository) {
        return Err(Error::new(format!(
            "{} is a checkout of {actual_repository:?}, but Cargo.toml lists {expected_repository:?}; choose another checkout name or move the existing directory",
            checkout.display()
        )));
    }

    Ok(())
}

fn normalize_repository(repository: &str) -> String {
    let repository = repository.trim_end_matches('/');
    let repository = repository.strip_suffix(".git").unwrap_or(repository);
    let github_path = repository
        .strip_prefix("https://github.com/")
        .or_else(|| repository.strip_prefix("http://github.com/"))
        .or_else(|| repository.strip_prefix("ssh://git@github.com/"))
        .or_else(|| repository.strip_prefix("git@github.com:"));

    if let Some(github_path) = github_path {
        format!("github.com/{github_path}")
    } else {
        repository.to_owned()
    }
}

pub(crate) fn ensure_local_patches_are_selected(
    checkout: &Path,
    local_packages: &[Package],
) -> Result<()> {
    let bytes = cargo::metadata_in(checkout, false)?;
    let metadata: Value = serde_json::from_slice(&bytes).map_err(|error| {
        Error::new(format!(
            "could not parse downstream Cargo metadata: {error}"
        ))
    })?;
    let packages = metadata
        .get("packages")
        .and_then(Value::as_array)
        .ok_or_else(|| Error::new("downstream Cargo metadata did not contain packages"))?;
    let package_names: std::collections::HashMap<_, _> = packages
        .iter()
        .filter_map(|package| {
            Some((
                package.get("id")?.as_str()?.to_owned(),
                package.get("name")?.as_str()?.to_owned(),
            ))
        })
        .collect();
    let local_package_identifiers: std::collections::HashMap<_, _> = local_packages
        .iter()
        .map(|package| (package.name.as_str(), package))
        .collect();
    let nodes = metadata
        .get("resolve")
        .and_then(|resolve| resolve.get("nodes"))
        .and_then(Value::as_array)
        .ok_or_else(|| Error::new("downstream Cargo metadata did not contain resolved nodes"))?;

    for node in nodes {
        let dependencies = node
            .get("deps")
            .and_then(Value::as_array)
            .into_iter()
            .flatten();
        for dependency in dependencies {
            let package_id = dependency
                .get("pkg")
                .and_then(Value::as_str)
                .ok_or_else(|| {
                    Error::new("Cargo dependency metadata did not contain a package identifier")
                })?;
            let Some(name) = package_names.get(package_id) else {
                continue;
            };
            if let Some(expected_package) = local_package_identifiers.get(name.as_str())
                && package_id != expected_package.package_identifier.as_str()
            {
                return Err(Error::new(format!(
                    "downstream dependency {name:?} did not resolve to this workspace's local package {} v{}; update the dependency requirement in {} to accept the local package version",
                    expected_package.name,
                    expected_package.version,
                    checkout.display()
                )));
            }
        }
    }

    Ok(())
}

pub(crate) fn run_cargo(checkout: &Path, arguments: &[String]) -> Result<()> {
    cargo::run_in(checkout, arguments)
}
