// Released under the MIT License.
// Copyright, 2026, by Samuel Williams.

use bake::{Error, Result};
use serde_json::Value;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

static REPORT_ID: AtomicU64 = AtomicU64::new(0);

#[derive(Debug)]
pub(crate) struct CoverageReport {
    directory: PathBuf,
    path: PathBuf,
}

impl CoverageReport {
    pub(crate) fn new() -> Result<Self> {
        Self::new_in(&std::env::temp_dir())
    }

    fn new_in(temporary_directory: &Path) -> Result<Self> {
        let identifier = REPORT_ID.fetch_add(1, Ordering::Relaxed);
        let directory = temporary_directory.join(format!(
            "bake-test-rust-coverage-{}-{identifier}",
            std::process::id()
        ));

        fs::create_dir(&directory).map_err(|error| {
            Error::new(format!(
                "failed to create temporary coverage report directory {}: {error}",
                directory.display()
            ))
        })?;

        let path = directory.join("coverage.json");
        Ok(Self { directory, path })
    }

    pub(crate) fn path(&self) -> &Path {
        &self.path
    }

    pub(crate) fn read(&self, source_root: &Path) -> Result<Summary> {
        let report = fs::read_to_string(&self.path).map_err(|error| {
            Error::new(format!(
                "failed to read coverage report {}: {error}",
                self.path.display()
            ))
        })?;

        summarize(&report, source_root)
    }
}

impl Drop for CoverageReport {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.directory);
    }
}

#[derive(Debug, PartialEq, Eq)]
pub(crate) struct Summary {
    raw_executable_lines: usize,
    raw_covered_lines: usize,
    excluded_lines: usize,
}

impl Summary {
    fn measured_executable_lines(&self) -> usize {
        self.raw_executable_lines - self.excluded_lines
    }

    fn uncovered_lines(&self) -> usize {
        self.measured_executable_lines() - self.raw_covered_lines
    }

    pub(crate) fn is_complete(&self) -> bool {
        self.uncovered_lines() == 0
    }

    pub(crate) fn success_message(&self) -> String {
        format!(
            "Line coverage passed: {}/{} measured lines covered (raw: {}/{}, excluded: {} unreachable lines)",
            self.raw_covered_lines,
            self.measured_executable_lines(),
            self.raw_covered_lines,
            self.raw_executable_lines,
            self.excluded_lines
        )
    }

    pub(crate) fn failure_message(&self) -> String {
        format!(
            "line coverage is {}/{} measured lines (raw: {}/{}, excluded: {} unreachable lines); {} measured lines remain uncovered",
            self.raw_covered_lines,
            self.measured_executable_lines(),
            self.raw_covered_lines,
            self.raw_executable_lines,
            self.excluded_lines,
            self.uncovered_lines()
        )
    }
}

fn summarize(report: &str, source_root: &Path) -> Result<Summary> {
    let json: Value = serde_json::from_str(report)
        .map_err(|error| Error::new(format!("invalid cargo-llvm-cov JSON report: {error}")))?;

    let data = json
        .get("data")
        .and_then(Value::as_array)
        .ok_or_else(|| Error::new("cargo-llvm-cov JSON report has no data array"))?;
    let mut raw_executable_lines = 0;
    let mut raw_covered_lines = 0;
    let mut excluded_lines = 0;
    let mut source_file_count = 0;

    for item in data {
        let files = item
            .get("files")
            .and_then(Value::as_array)
            .ok_or_else(|| Error::new("cargo-llvm-cov JSON report has no files array"))?;

        for file in files {
            source_file_count += 1;
            let reported_path = file
                .get("filename")
                .and_then(Value::as_str)
                .ok_or_else(|| Error::new("coverage file has no filename"))?;
            let reported_path = PathBuf::from(reported_path);
            let source_path = resolve_source_path(&reported_path, source_root);
            let source = fs::read_to_string(&source_path).map_err(|error| {
                Error::new(format!(
                    "failed to read covered source file {}: {error}",
                    source_path.display()
                ))
            })?;
            let line_summary = file
                .get("summary")
                .and_then(|summary| summary.get("lines"))
                .ok_or_else(|| Error::new("coverage file has no line summary"))?;
            let executable_lines = json_usize(line_summary, "count", "line summary")?;
            let covered_lines = json_usize(line_summary, "covered", "line summary")?;
            let segments = file
                .get("segments")
                .and_then(Value::as_array)
                .ok_or_else(|| Error::new("coverage file has no segments array"))?;
            let mut file_excluded_lines = 0;

            for (index, line) in source.lines().enumerate() {
                if is_single_line_unreachable_expression(line)
                    && is_uncovered_executable_line(segments, index + 1)?
                {
                    file_excluded_lines += 1;
                    excluded_lines += 1;
                }
            }

            if executable_lines < file_excluded_lines {
                return Err(Error::new(format!(
                    "coverage file {} reports fewer executable lines than unreachable exclusions",
                    source_path.display()
                )));
            }
            if covered_lines > executable_lines {
                return Err(Error::new(format!(
                    "coverage file {} reports more covered lines than executable lines",
                    source_path.display()
                )));
            }

            raw_executable_lines += executable_lines;
            raw_covered_lines += covered_lines;
        }
    }

    if source_file_count == 0 {
        return Err(Error::new(
            "cargo-llvm-cov JSON report contains no source files",
        ));
    }

    if raw_executable_lines - raw_covered_lines < excluded_lines {
        return Err(Error::new(
            "unreachable lines exceed the number of uncovered executable lines",
        ));
    }

    Ok(Summary {
        raw_executable_lines,
        raw_covered_lines,
        excluded_lines,
    })
}

fn json_usize(value: &Value, key: &str, description: &str) -> Result<usize> {
    value
        .get(key)
        .and_then(Value::as_u64)
        .and_then(|value| usize::try_from(value).ok())
        .ok_or_else(|| Error::new(format!("coverage {description} has no valid {key}")))
}

fn resolve_source_path(reported_path: &Path, source_root: &Path) -> PathBuf {
    if reported_path.is_absolute() {
        reported_path.to_owned()
    } else {
        source_root.join(reported_path)
    }
}

fn is_uncovered_executable_line(segments: &[Value], number: usize) -> Result<bool> {
    let mut has_code_region = false;
    let mut is_covered = false;

    for segment in segments {
        let fields = segment
            .as_array()
            .filter(|fields| fields.len() >= 6)
            .ok_or_else(|| Error::new("coverage report contains an invalid source segment"))?;
        let line = fields[0]
            .as_u64()
            .and_then(|line| usize::try_from(line).ok())
            .ok_or_else(|| Error::new("coverage segment has an invalid source line"))?;
        if line != number {
            continue;
        }

        let count = fields[2]
            .as_u64()
            .ok_or_else(|| Error::new("coverage segment has an invalid execution count"))?;
        let has_count = fields[3]
            .as_bool()
            .ok_or_else(|| Error::new("coverage segment has no has-count value"))?;
        let is_gap_region = fields[5]
            .as_bool()
            .ok_or_else(|| Error::new("coverage segment has no gap-region value"))?;

        if has_count && !is_gap_region {
            has_code_region = true;
            if count > 0 {
                is_covered = true;
            }
        }
    }

    Ok(has_code_region && !is_covered)
}

fn is_single_line_unreachable_expression(line: &str) -> bool {
    let Some(macro_start) = line.find("unreachable!") else {
        return false;
    };
    let expression_prefix = line[..macro_start].trim();
    if !expression_prefix.is_empty()
        && (!expression_prefix.ends_with("=>") || expression_prefix.matches("=>").count() != 1)
    {
        return false;
    }

    let invocation = &line[macro_start + "unreachable!".len()..];
    let Some(open_parenthesis) = invocation.find('(') else {
        return false;
    };
    let Some(close_parenthesis) = invocation.rfind(')') else {
        return false;
    };
    if close_parenthesis <= open_parenthesis
        || invocation[open_parenthesis + 1..close_parenthesis]
            .trim()
            .is_empty()
    {
        return false;
    }

    let expression_suffix = invocation[close_parenthesis + 1..].trim();
    expression_suffix.is_empty()
        || expression_suffix == ";"
        || expression_suffix == ","
        || expression_suffix.starts_with("//")
}

#[cfg(test)]
#[path = "coverage/tests.rs"]
mod tests;
