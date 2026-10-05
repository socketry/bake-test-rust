// Released under the MIT License.
// Copyright, 2026, by Samuel Williams.

use bake::{Error, Result};
use serde_json::Value;
use std::collections::{HashMap, HashSet};
use std::fs;
use std::ops::Range;
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

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
struct Position {
    line: usize,
    column: usize,
}

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
struct SourceRegion {
    path: PathBuf,
    start: Position,
    end: Position,
    kind: usize,
}

#[derive(Debug, PartialEq, Eq)]
pub(crate) struct Summary {
    covered_regions: usize,
    measured_regions: usize,
    uncovered_regions: Vec<SourceRegion>,
    excluded_unreachable_regions: usize,
}

impl Summary {
    pub(crate) fn is_complete(&self) -> bool {
        self.uncovered_regions.is_empty()
    }

    pub(crate) fn success_message(&self) -> String {
        format!(
            "Region coverage passed: {}/{} measured source regions covered (excluded: {} unreachable regions)",
            self.covered_regions, self.measured_regions, self.excluded_unreachable_regions,
        )
    }

    pub(crate) fn failure_message(&self) -> String {
        let mut message = format!(
            "region coverage is {}/{} measured source regions (excluded: {} unreachable regions); {} source regions remain uncovered",
            self.covered_regions,
            self.measured_regions,
            self.excluded_unreachable_regions,
            self.uncovered_regions.len(),
        );

        for region in &self.uncovered_regions {
            message.push_str(&format!(
                "\n  {}:{}:{}-{}:{}",
                region.path.display(),
                region.start.line,
                region.start.column,
                region.end.line,
                region.end.column,
            ));
        }

        message
    }
}

#[derive(Debug)]
struct SourceFile {
    text: String,
    line_starts: Vec<usize>,
    unreachable_macros: Vec<Range<usize>>,
}

impl SourceFile {
    fn read(path: &Path) -> Result<Self> {
        let text = fs::read_to_string(path).map_err(|error| {
            Error::new(format!(
                "failed to read covered source file {}: {error}",
                path.display()
            ))
        })?;

        Ok(Self::new(text))
    }

    fn new(text: String) -> Self {
        let mut line_starts = vec![0];
        for (index, byte) in text.bytes().enumerate() {
            if byte == b'\n' {
                line_starts.push(index + 1);
            }
        }

        let unreachable_macros = find_unreachable_macros(&text);

        Self {
            text,
            line_starts,
            unreachable_macros,
        }
    }

    fn offset(&self, position: Position) -> Result<usize> {
        if position.line == 0 || position.column == 0 {
            return Err(Error::new("coverage region has a zero source position"));
        }

        let Some(&line_start) = self.line_starts.get(position.line - 1) else {
            return Err(Error::new(
                "coverage region starts past the end of its source file",
            ));
        };

        let line_end = self
            .line_starts
            .get(position.line)
            .copied()
            .map(|next_line_start| next_line_start - 1)
            .unwrap_or(self.text.len());
        let offset = line_start
            .checked_add(position.column - 1)
            .ok_or_else(|| Error::new("coverage region column exceeds its source line"))?;
        if offset > line_end {
            return Err(Error::new("coverage region column exceeds its source line"));
        }

        Ok(offset)
    }

    fn span(&self, region: &SourceRegion) -> Result<Range<usize>> {
        let start = self.offset(region.start)?;
        let end = self.offset(region.end)?;
        if end < start {
            return Err(Error::new("coverage region ends before it starts"));
        }
        if self.text.get(start..end).is_none() {
            return Err(Error::new(
                "coverage region does not align with source text",
            ));
        }

        Ok(start..end)
    }

    fn is_unreachable(&self, span: &Range<usize>) -> bool {
        self.unreachable_macros
            .iter()
            .any(|macro_span| span.start >= macro_span.start && span.end <= macro_span.end)
    }
}

fn summarize(report: &str, source_root: &Path) -> Result<Summary> {
    let json: Value = serde_json::from_str(report)
        .map_err(|error| Error::new(format!("invalid cargo-llvm-cov JSON report: {error}")))?;

    let data = json
        .get("data")
        .and_then(Value::as_array)
        .ok_or_else(|| Error::new("cargo-llvm-cov JSON report has no data array"))?;
    let mut sources = HashMap::new();
    let mut regions = HashMap::<SourceRegion, bool>::new();
    let mut report_file_count = 0;
    let mut region_record_count = 0;

    for item in data {
        let files = item
            .get("files")
            .and_then(Value::as_array)
            .ok_or_else(|| Error::new("cargo-llvm-cov JSON report has no files array"))?;
        let mut item_paths = HashSet::new();

        for file in files {
            report_file_count += 1;
            let reported_path = file
                .get("filename")
                .and_then(Value::as_str)
                .ok_or_else(|| Error::new("coverage file has no filename"))?;
            let path = resolve_source_path(Path::new(reported_path), source_root);
            item_paths.insert(path.clone());
            if !sources.contains_key(&path) {
                sources.insert(path.clone(), SourceFile::read(&path)?);
            }
        }

        let functions = item
            .get("functions")
            .and_then(Value::as_array)
            .ok_or_else(|| Error::new("cargo-llvm-cov JSON report has no functions array"))?;

        for function in functions {
            let filenames = function
                .get("filenames")
                .and_then(Value::as_array)
                .ok_or_else(|| Error::new("coverage function has no filenames array"))?;
            let function_regions = function
                .get("regions")
                .and_then(Value::as_array)
                .ok_or_else(|| Error::new("coverage function has no regions array"))?;

            for region in function_regions {
                let fields = region
                    .as_array()
                    .filter(|fields| fields.len() >= 8)
                    .ok_or_else(|| {
                        Error::new("coverage report contains an invalid source region")
                    })?;
                let file_id = json_usize_field(fields, 5, "source region file id")?;
                let Some(filename) = filenames.get(file_id).and_then(Value::as_str) else {
                    return Err(Error::new("coverage source region has an invalid file id"));
                };
                let path = resolve_source_path(Path::new(filename), source_root);
                if !item_paths.contains(&path) {
                    continue;
                }

                let kind = json_usize_field(fields, 7, "source region kind")?;
                if kind != 0 && kind != 1 {
                    continue;
                }

                region_record_count += 1;
                let region = SourceRegion {
                    path,
                    start: Position {
                        line: json_usize_field(fields, 0, "source region start line")?,
                        column: json_usize_field(fields, 1, "source region start column")?,
                    },
                    end: Position {
                        line: json_usize_field(fields, 2, "source region end line")?,
                        column: json_usize_field(fields, 3, "source region end column")?,
                    },
                    kind,
                };
                let count = json_usize_field(fields, 4, "source region execution count")?;
                regions
                    .entry(region)
                    .and_modify(|covered| *covered |= count > 0)
                    .or_insert(count > 0);
            }
        }
    }

    if report_file_count == 0 {
        return Err(Error::new(
            "cargo-llvm-cov JSON report contains no source files",
        ));
    }
    if region_record_count == 0 {
        return Err(Error::new(
            "cargo-llvm-cov JSON report contains no source regions",
        ));
    }

    let mut summary = Summary {
        covered_regions: 0,
        measured_regions: 0,
        uncovered_regions: Vec::new(),
        excluded_unreachable_regions: 0,
    };

    for (region, covered) in regions {
        let Some(source) = sources.get(&region.path) else {
            unreachable!("coverage regions must refer to reported source files");
        };
        let span = source.span(&region)?;
        if source.is_unreachable(&span) {
            summary.excluded_unreachable_regions += 1;
        } else {
            summary.measured_regions += 1;
            if covered {
                summary.covered_regions += 1;
            } else {
                summary.uncovered_regions.push(region);
            }
        }
    }

    summary.uncovered_regions.sort_by(compare_regions);

    Ok(summary)
}

fn compare_regions(left: &SourceRegion, right: &SourceRegion) -> std::cmp::Ordering {
    left.path
        .cmp(&right.path)
        .then_with(|| left.start.line.cmp(&right.start.line))
        .then_with(|| left.start.column.cmp(&right.start.column))
        .then_with(|| left.end.line.cmp(&right.end.line))
        .then_with(|| left.end.column.cmp(&right.end.column))
        .then_with(|| left.kind.cmp(&right.kind))
}

fn json_usize_field(fields: &[Value], index: usize, description: &str) -> Result<usize> {
    fields
        .get(index)
        .and_then(Value::as_u64)
        .and_then(|value| usize::try_from(value).ok())
        .ok_or_else(|| Error::new(format!("coverage {description} is invalid")))
}

fn resolve_source_path(reported_path: &Path, source_root: &Path) -> PathBuf {
    let path = if reported_path.is_absolute() {
        reported_path.to_owned()
    } else {
        source_root.join(reported_path)
    };

    fs::canonicalize(&path).unwrap_or(path)
}

/// Finds `unreachable!()` invocations in Rust source already validated by the compiler.
fn find_unreachable_macros(source: &str) -> Vec<Range<usize>> {
    let mut ranges = Vec::new();
    let mut index = 0;

    while index < source.len() {
        if let Some(next) = skip_non_code(source, index) {
            index = next;
            continue;
        }

        if source[index..].starts_with("unreachable")
            && is_identifier_boundary_before(source, index)
        {
            let name_end = index + "unreachable".len();
            if is_identifier_boundary_after(source, name_end) {
                let bang = skip_trivia(source, name_end);
                if source.as_bytes().get(bang) == Some(&b'!') {
                    let opening = skip_trivia(source, bang + 1);
                    let Some(&delimiter) = source.as_bytes().get(opening) else {
                        unreachable!("compiled Rust macro invocations have an opening delimiter");
                    };
                    let closing = matching_group_end(source, opening, delimiter);
                    ranges.push(macro_path_start(source, index)..closing);
                    index = closing;
                    continue;
                }
            }
        }

        index += source[index..].chars().next().map_or(1, char::len_utf8);
    }

    ranges
}

fn is_identifier_boundary_before(source: &str, index: usize) -> bool {
    index == 0 || !is_identifier_byte(source.as_bytes()[index - 1])
}

fn is_identifier_boundary_after(source: &str, index: usize) -> bool {
    source
        .as_bytes()
        .get(index)
        .is_none_or(|byte| !is_identifier_byte(*byte))
}

fn is_identifier_byte(byte: u8) -> bool {
    byte == b'_' || byte.is_ascii_alphanumeric() || byte >= 0x80
}

fn skip_trivia(source: &str, mut index: usize) -> usize {
    loop {
        while source
            .as_bytes()
            .get(index)
            .is_some_and(u8::is_ascii_whitespace)
        {
            index += 1;
        }

        let Some(next) = skip_comment(source, index) else {
            return index;
        };
        index = next;
    }
}

fn skip_non_code(source: &str, index: usize) -> Option<usize> {
    if let Some(next) = skip_comment(source, index) {
        return Some(next);
    }
    if let Some(next) = skip_raw_string(source, index) {
        return Some(next);
    }
    if let Some(next) = skip_quoted_string(source, index) {
        return Some(next);
    }
    skip_character_literal(source, index)
}

fn skip_comment(source: &str, index: usize) -> Option<usize> {
    let bytes = source.as_bytes();
    if bytes.get(index..index + 2) == Some(b"//") {
        return Some(
            bytes[index..]
                .iter()
                .position(|byte| *byte == b'\n')
                .map_or(source.len(), |offset| index + offset + 1),
        );
    }
    if bytes.get(index..index + 2) != Some(b"/*") {
        return None;
    }

    let mut depth = 1;
    let mut cursor = index + 2;
    while cursor < bytes.len() {
        if bytes.get(cursor..cursor + 2) == Some(b"/*") {
            depth += 1;
            cursor += 2;
        } else if bytes.get(cursor..cursor + 2) == Some(b"*/") {
            depth -= 1;
            cursor += 2;
            if depth == 0 {
                return Some(cursor);
            }
        } else {
            cursor += 1;
        }
    }

    unreachable!("compiled Rust source has closed block comments");
}

fn skip_raw_string(source: &str, index: usize) -> Option<usize> {
    let bytes = source.as_bytes();
    let raw_prefix = if bytes.get(index..index + 2) == Some(b"br")
        || bytes.get(index..index + 2) == Some(b"cr")
    {
        index + 1
    } else if bytes.get(index) == Some(&b'r') {
        index
    } else {
        return None;
    };

    let mut quote = raw_prefix + 1;
    while bytes.get(quote) == Some(&b'#') {
        quote += 1;
    }
    if bytes.get(quote) != Some(&b'"') {
        return None;
    }

    let hashes = quote - raw_prefix - 1;
    let mut cursor = quote + 1;
    while cursor < bytes.len() {
        if bytes[cursor] == b'"'
            && bytes
                .get(cursor + 1..cursor + 1 + hashes)
                .is_some_and(|suffix| suffix.iter().all(|byte| *byte == b'#'))
        {
            return Some(cursor + 1 + hashes);
        }
        cursor += 1;
    }

    unreachable!("compiled Rust source has closed raw strings");
}

fn skip_quoted_string(source: &str, index: usize) -> Option<usize> {
    let bytes = source.as_bytes();
    let quote = match bytes.get(index) {
        Some(b'"') => index,
        Some(b'b' | b'c') if bytes.get(index + 1) == Some(&b'"') => index + 1,
        _ => return None,
    };

    let mut cursor = quote + 1;
    while cursor < bytes.len() {
        match bytes[cursor] {
            b'\\' => cursor = (cursor + 2).min(bytes.len()),
            b'"' => return Some(cursor + 1),
            _ => cursor += 1,
        }
    }

    unreachable!("compiled Rust source has closed strings");
}

fn skip_character_literal(source: &str, index: usize) -> Option<usize> {
    let bytes = source.as_bytes();
    let quote = match bytes.get(index) {
        Some(b'\'') => index,
        Some(b'b') if bytes.get(index + 1) == Some(&b'\'') => index + 1,
        _ => return None,
    };

    let mut cursor = quote + 1;
    while cursor < bytes.len() && bytes[cursor] != b'\n' {
        match bytes[cursor] {
            b'\\' => cursor = (cursor + 2).min(bytes.len()),
            b'\'' => return Some(cursor + 1),
            _ => cursor += 1,
        }
    }

    None
}

fn matching_group_end(source: &str, opening: usize, delimiter: u8) -> usize {
    let closing = match delimiter {
        b'(' => b')',
        b'[' => b']',
        b'{' => b'}',
        _ => unreachable!("macro invocation starts with a group delimiter"),
    };
    let mut stack = vec![closing];
    let mut index = opening + 1;

    while index < source.len() {
        if let Some(next) = skip_non_code(source, index) {
            index = next;
            continue;
        }

        match source.as_bytes()[index] {
            b'(' => stack.push(b')'),
            b'[' => stack.push(b']'),
            b'{' => stack.push(b'}'),
            b')' | b']' | b'}' => {
                if stack.pop() != Some(source.as_bytes()[index]) {
                    unreachable!("compiled Rust macro token trees have balanced delimiters");
                }
                if stack.is_empty() {
                    return index + 1;
                }
            }
            _ => {}
        }

        index += source[index..].chars().next().map_or(1, char::len_utf8);
    }

    unreachable!("compiled Rust macro token trees have closing delimiters");
}

fn macro_path_start(source: &str, index: usize) -> usize {
    let bytes = source.as_bytes();
    let mut start = index;
    loop {
        if start >= 2 && bytes.get(start - 2..start) == Some(b"r#") {
            return start - 2;
        }

        if start < 2 || bytes.get(start - 2..start) != Some(b"::") {
            return start;
        }

        let mut segment_start = start - 2;
        while segment_start > 0 && is_identifier_byte(bytes[segment_start - 1]) {
            segment_start -= 1;
        }
        if segment_start == start - 2 {
            return start - 2;
        }
        start = segment_start;
    }
}

#[cfg(test)]
#[path = "coverage/tests.rs"]
mod tests;
