// Released under the MIT License.
// Copyright, 2026, by Samuel Williams.

use bake::{Context, Error, Result};
use serde_json::Value;
use std::path::{Path, PathBuf};

use crate::cargo;

#[derive(Clone, Debug)]
pub(crate) struct Package {
    pub package_identifier: String,
    pub name: String,
    pub version: String,
    pub manifest_path: PathBuf,
}

#[derive(Clone, Debug)]
pub(crate) struct ExternalRepository {
    pub repository: String,
    pub branch: String,
    pub name: String,
}

#[derive(Clone, Debug)]
pub(crate) struct Workspace {
    pub packages: Vec<Package>,
    pub external_repositories: Vec<ExternalRepository>,
}

pub(crate) fn read_workspace(context: &Context) -> Result<Workspace> {
    let output = cargo::metadata_in(context.root(), true)?;
    let metadata: Value = serde_json::from_slice(&output)
        .map_err(|error| Error::new(format!("could not parse Cargo metadata: {error}")))?;

    parse_workspace(&metadata)
}

fn parse_workspace(metadata: &Value) -> Result<Workspace> {
    let root = metadata
        .get("workspace_root")
        .and_then(Value::as_str)
        .map(PathBuf::from)
        .ok_or_else(|| Error::new("Cargo metadata did not contain a workspace root"))?;
    let workspace_members = metadata
        .get("workspace_members")
        .and_then(Value::as_array)
        .ok_or_else(|| Error::new("Cargo metadata did not contain workspace members"))?;
    let workspace_member_package_identifiers: Vec<_> = workspace_members
        .iter()
        .map(|member| {
            member
                .as_str()
                .ok_or_else(|| Error::new("Cargo metadata workspace member was not a string"))
        })
        .collect::<Result<_>>()?;
    let package_values = metadata
        .get("packages")
        .and_then(Value::as_array)
        .ok_or_else(|| Error::new("Cargo metadata did not contain packages"))?;

    let mut packages = Vec::new();
    for package in package_values {
        let package_identifier = required_string(package, "id", "Cargo package")?;
        if !workspace_member_package_identifiers.contains(&package_identifier.as_str())
            || !publishable_to_crates_io(package)
        {
            continue;
        }
        packages.push(Package {
            package_identifier,
            name: required_string(package, "name", "Cargo package")?,
            version: required_string(package, "version", "Cargo package")?,
            manifest_path: PathBuf::from(required_string(
                package,
                "manifest_path",
                "Cargo package",
            )?),
        });
    }
    packages.sort_by(|left, right| left.name.cmp(&right.name));
    for pair in packages.windows(2) {
        if pair[0].name == pair[1].name {
            return Err(Error::new(format!(
                "workspace contains more than one crates.io package named {:?}; external tests need one local package per crates.io name",
                pair[0].name
            )));
        }
    }

    let external_repositories = read_external_repositories(metadata, &root)?;

    Ok(Workspace {
        packages,
        external_repositories,
    })
}

pub(crate) fn read_external_repositories(
    metadata: &Value,
    workspace_root: &Path,
) -> Result<Vec<ExternalRepository>> {
    let external = metadata
        .get("metadata")
        .and_then(|value| value.get("bake"))
        .and_then(|value| value.get("test"))
        .and_then(|value| value.get("external"))
        .or_else(|| root_package_metadata(metadata, workspace_root));

    let Some(external) = external else {
        return Ok(Vec::new());
    };
    let entries = external
        .as_array()
        .ok_or_else(|| Error::new("Cargo metadata bake.test.external must be an array"))?;

    let mut repositories = Vec::with_capacity(entries.len());
    for entry in entries {
        let repository = required_string(entry, "repository", "external test entry")?;
        let branch = entry
            .get("branch")
            .map(|branch| {
                branch.as_str().map(str::to_owned).ok_or_else(|| {
                    Error::new("Cargo metadata external test branch must be a string")
                })
            })
            .transpose()?
            .unwrap_or_else(|| "main".to_owned());
        validate_branch(&branch)?;

        let derived_name = repository_name(&repository)?;
        let name = entry
            .get("name")
            .map(|name| {
                name.as_str()
                    .map(str::to_owned)
                    .ok_or_else(|| Error::new("Cargo metadata external test name must be a string"))
            })
            .transpose()?
            .unwrap_or(derived_name);
        validate_checkout_name(&name)?;

        repositories.push(ExternalRepository {
            repository: clone_url(&repository)?,
            branch,
            name,
        });
    }

    repositories.sort_by(|left, right| left.name.cmp(&right.name));
    for pair in repositories.windows(2) {
        if pair[0].name == pair[1].name {
            return Err(Error::new(format!(
                "external repositories use the same checkout name {:?}; set distinct name values in Cargo.toml",
                pair[0].name
            )));
        }
    }

    Ok(repositories)
}

fn root_package_metadata<'a>(metadata: &'a Value, workspace_root: &Path) -> Option<&'a Value> {
    let root_manifest = workspace_root.join("Cargo.toml");
    metadata
        .get("packages")?
        .as_array()?
        .iter()
        .find(|package| {
            package
                .get("manifest_path")
                .and_then(Value::as_str)
                .is_some_and(|manifest_path| Path::new(manifest_path) == root_manifest)
        })?
        .get("metadata")?
        .get("bake")?
        .get("test")?
        .get("external")
}

fn publishable_to_crates_io(package: &Value) -> bool {
    match package.get("publish") {
        None | Some(Value::Null) | Some(Value::Bool(true)) => true,
        Some(Value::Bool(false)) => false,
        Some(Value::Array(registries)) => registries
            .iter()
            .any(|registry| registry.as_str() == Some("crates-io")),
        Some(_) => false,
    }
}

fn required_string(value: &Value, key: &str, description: &str) -> Result<String> {
    value
        .get(key)
        .and_then(Value::as_str)
        .filter(|value| !value.is_empty())
        .map(str::to_owned)
        .ok_or_else(|| Error::new(format!("{description} must contain a nonempty {key}")))
}

fn clone_url(repository: &str) -> Result<String> {
    if repository.starts_with("https://")
        || repository.starts_with("http://")
        || repository.starts_with("ssh://")
        || repository.starts_with("git://")
        || repository.starts_with("git@")
        || repository.starts_with("file://")
    {
        return Ok(repository.to_owned());
    }

    let Some((owner, name)) = repository.split_once('/') else {
        return Err(Error::new(
            "external repository must be a Git URL or use owner/repository format",
        ));
    };
    if owner.is_empty() || name.is_empty() || name.contains('/') || owner.starts_with('-') {
        return Err(Error::new(
            "external repository has an invalid owner/repository name",
        ));
    }
    Ok(format!("https://github.com/{owner}/{name}"))
}

fn repository_name(repository: &str) -> Result<String> {
    let trimmed = repository.trim_end_matches('/').trim_end_matches(".git");
    let name = trimmed
        .rsplit(['/', ':'])
        .next()
        .filter(|name| !name.is_empty())
        .ok_or_else(|| Error::new("could not derive a checkout name from repository URL"))?;
    validate_checkout_name(name)?;
    Ok(name.to_owned())
}

fn validate_checkout_name(name: &str) -> Result<()> {
    if name.is_empty()
        || name == "."
        || name == ".."
        || !name
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'_' | b'-'))
    {
        return Err(Error::new(
            "external checkout names may contain only letters, numbers, dots, underscores, and hyphens",
        ));
    }
    Ok(())
}

fn validate_branch(branch: &str) -> Result<()> {
    if branch.is_empty() || branch.starts_with('-') || branch.chars().any(char::is_control) {
        return Err(Error::new(
            "external test branch must be nonempty and cannot start with a hyphen or contain control characters",
        ));
    }
    Ok(())
}

#[cfg(test)]
#[path = "metadata/tests.rs"]
mod tests;
