// Released under the MIT License.
// Copyright, 2026, by Samuel Williams.

use super::{CoverageReport, is_single_line_unreachable_expression, summarize};
use crate::test_support::TemporaryDirectory;
use serde_json::{Value, json};
use std::fs;
use std::path::Path;

fn source_and_report(source: &str, line_data: &[(usize, u64)]) -> (TemporaryDirectory, String) {
    let directory = TemporaryDirectory::new();
    let source_path = directory.path().join("src/lib.rs");
    fs::create_dir_all(source_path.parent().expect("source parent"))
        .expect("create source directory");
    fs::write(&source_path, source).expect("write source file");

    let report = report_for_path(&source_path, line_data);

    (directory, report)
}

fn report_for_path(source_path: &Path, line_data: &[(usize, u64)]) -> String {
    let covered = line_data.iter().filter(|(_, count)| *count > 0).count();
    let segments = line_data
        .iter()
        .map(|(number, count)| json!([number, 1, count, true, true, false]))
        .collect::<Vec<_>>();

    report_for_path_with_data(source_path, line_data.len(), covered, segments)
}

fn report_for_path_with_data(
    source_path: &Path,
    executable_lines: usize,
    covered_lines: usize,
    segments: Vec<Value>,
) -> String {
    json!({
        "data": [{
            "files": [{
                "filename": source_path,
                "summary": {"lines": {"count": executable_lines, "covered": covered_lines}},
                "segments": segments,
            }]
        }]
    })
    .to_string()
}

fn json_with_files(files: Value) -> String {
    json!({"data": [{"files": files}]}).to_string()
}

#[test]
fn excludes_only_uncovered_unreachable_lines() {
    let (directory, report) = source_and_report(
        "fn live() {}\n_ => unreachable!(\"Only JSX events can be mismatched here\")\n_ => unreachable!(\"Another invariant\")\n",
        &[(1, 1), (2, 0), (3, 0)],
    );

    let summary = summarize(&report, directory.path()).expect("summarize coverage");

    assert!(summary.is_complete());
    assert_eq!(summary.raw_executable_lines, 3);
    assert_eq!(summary.raw_covered_lines, 1);
    assert_eq!(summary.measured_executable_lines(), 1);
    assert_eq!(summary.excluded_lines, 2);
    assert!(summary.success_message().contains("raw: 1/3, excluded: 2"));
}

#[test]
fn recognizes_only_complete_reasoned_unreachable_calls() {
    assert!(is_single_line_unreachable_expression(
        "unreachable!(\"reason\")"
    ));
    assert!(!is_single_line_unreachable_expression("unreachable!"));
    assert!(!is_single_line_unreachable_expression(
        "unreachable!(\"reason\""
    ));
}

#[test]
fn reports_uncovered_live_lines() {
    let (directory, report) = source_and_report(
        "fn live() {}\npanic!(\"live failure path\")\n",
        &[(1, 1), (2, 0)],
    );

    let summary = summarize(&report, directory.path()).expect("summarize coverage");

    assert!(!summary.is_complete());
    assert_eq!(summary.uncovered_lines(), 1);
    assert!(
        summary
            .failure_message()
            .contains("1 measured lines remain uncovered")
    );
}

#[test]
fn rejects_non_json_reports() {
    let directory = TemporaryDirectory::new();

    assert!(
        summarize("{", directory.path())
            .expect_err("invalid JSON should fail")
            .to_string()
            .contains("invalid cargo-llvm-cov JSON report")
    );
}

#[test]
fn rejects_reports_without_data_or_files() {
    let directory = TemporaryDirectory::new();

    assert!(
        summarize("{}", directory.path())
            .expect_err("report requires data")
            .to_string()
            .contains("no data array")
    );
    assert!(
        summarize(&json_with_files(json!({})), directory.path())
            .expect_err("report requires files")
            .to_string()
            .contains("no files array")
    );
    assert!(
        summarize(&json_with_files(json!([])), directory.path())
            .expect_err("report requires source files")
            .to_string()
            .contains("contains no source files")
    );
    assert!(
        summarize(&json_with_files(json!(null)), directory.path())
            .expect_err("files must be an array")
            .to_string()
            .contains("no files array")
    );
}

#[test]
fn rejects_coverage_files_without_required_metadata() {
    let directory = TemporaryDirectory::new();

    assert!(
        summarize(&json_with_files(json!([{}])), directory.path())
            .expect_err("file requires a filename")
            .to_string()
            .contains("no filename")
    );

    let source_path = directory.path().join("src/lib.rs");
    fs::create_dir_all(source_path.parent().expect("source parent"))
        .expect("create source directory");
    fs::write(&source_path, "fn live() {}\n").expect("write source");

    let report = json_with_files(json!([{
        "filename": source_path.clone(),
        "segments": [],
    }]));
    assert!(
        summarize(&report, directory.path())
            .expect_err("file requires a line summary")
            .to_string()
            .contains("no line summary")
    );

    let report = json_with_files(json!([{
        "filename": source_path.clone(),
        "summary": {"lines": {"count": 1, "covered": 1}},
    }]));
    assert!(
        summarize(&report, directory.path())
            .expect_err("file requires segments")
            .to_string()
            .contains("no segments array")
    );
}

#[test]
fn rejects_invalid_line_summary_counts() {
    let directory = TemporaryDirectory::new();
    let source_path = directory.path().join("src/lib.rs");
    fs::create_dir_all(source_path.parent().expect("source parent"))
        .expect("create source directory");
    fs::write(&source_path, "fn live() {}\n").expect("write source");

    let report = json_with_files(json!([{
        "filename": source_path,
        "summary": {"lines": {"count": "invalid", "covered": 0}},
        "segments": [],
    }]));
    assert!(
        summarize(&report, directory.path())
            .expect_err("line count must be numeric")
            .to_string()
            .contains("no valid count")
    );

    let report = json_with_files(json!([{
        "filename": source_path,
        "summary": {"lines": {"count": 1, "covered": "invalid"}},
        "segments": [],
    }]));
    assert!(
        summarize(&report, directory.path())
            .expect_err("covered count must be numeric")
            .to_string()
            .contains("no valid covered")
    );
}

#[test]
fn rejects_inconsistent_file_line_summaries() {
    let (directory, _) = source_and_report("unreachable!(\"invariant\")\n", &[(1, 0)]);
    let source_path = directory.path().join("src/lib.rs");
    let zero_executable = report_for_path_with_data(
        &source_path,
        0,
        0,
        vec![json!([1, 1, 0, true, true, false])],
    );
    assert!(
        summarize(&zero_executable, directory.path())
            .expect_err("unreachable exclusion exceeds executable count")
            .to_string()
            .contains("fewer executable lines")
    );

    let second_source_path = directory.path().join("src/other.rs");
    fs::write(&second_source_path, "fn live() {}\n").expect("write second source");
    let too_many_covered = report_for_path_with_data(&second_source_path, 0, 1, vec![]);
    assert!(
        summarize(&too_many_covered, directory.path())
            .expect_err("covered count exceeds executable count")
            .to_string()
            .contains("more covered lines")
    );
}

#[test]
fn rejects_more_unreachable_exclusions_than_reported_uncovered_lines() {
    let (directory, _) = source_and_report("unreachable!(\"invariant\")\n", &[(1, 0)]);
    let source_path = directory.path().join("src/lib.rs");
    let report = report_for_path_with_data(
        &source_path,
        1,
        1,
        vec![json!([1, 1, 0, true, true, false])],
    );

    assert!(
        summarize(&report, directory.path())
            .expect_err("unreachable exclusion must correspond to a reported uncovered line")
            .to_string()
            .contains("unreachable lines exceed the number of uncovered")
    );
}

#[test]
fn accepts_relative_source_paths() {
    let (directory, report) = source_and_report("fn live() {}\n", &[(1, 1)]);
    let source_path = directory.path().join("src/lib.rs");
    let report = report.replace(&source_path.display().to_string(), "src/lib.rs");

    let summary = summarize(&report, directory.path()).expect("resolve relative source path");

    assert!(summary.is_complete());
    assert_eq!(summary.raw_executable_lines, 1);
}

#[test]
fn reports_source_file_read_errors() {
    let directory = TemporaryDirectory::new();
    let report = report_for_path(&directory.path().join("missing.rs"), &[(1, 1)]);

    assert!(
        summarize(&report, directory.path())
            .expect_err("missing source should fail")
            .to_string()
            .contains("failed to read covered source file")
    );
}

#[test]
fn does_not_exclude_unreachable_macro_text_inside_a_string() {
    let (directory, report) = source_and_report("let message = \"unreachable!\";\n", &[(1, 0)]);

    let summary = summarize(&report, directory.path()).expect("report uncovered line");

    assert!(!summary.is_complete());
    assert_eq!(summary.excluded_lines, 0);
}

#[test]
fn does_not_exclude_multiple_match_arms_on_one_line() {
    let (directory, report) =
        source_and_report("A => value, B => unreachable!(\"reason\")\n", &[(1, 0)]);

    let summary = summarize(&report, directory.path()).expect("report uncovered line");

    assert!(!summary.is_complete());
    assert_eq!(summary.excluded_lines, 0);
}

#[test]
fn does_not_exclude_unreachable_lines_on_gap_segments() {
    let (directory, _) = source_and_report("unreachable!(\"invariant\")\n", &[(1, 0)]);
    let source_path = directory.path().join("src/lib.rs");
    let report =
        report_for_path_with_data(&source_path, 1, 0, vec![json!([1, 1, 0, true, true, true])]);

    let summary = summarize(&report, directory.path()).expect("report uncovered line");

    assert!(!summary.is_complete());
    assert_eq!(summary.excluded_lines, 0);
}

#[test]
fn does_not_exclude_unreachable_calls_with_trailing_expressions() {
    let (directory, report) = source_and_report("unreachable!(\"reason\") + 1\n", &[(1, 0)]);

    let summary = summarize(&report, directory.path()).expect("report uncovered line");

    assert!(!summary.is_complete());
    assert_eq!(summary.excluded_lines, 0);
}

#[test]
fn does_not_exclude_unreachable_calls_without_a_reason() {
    let (directory, report) = source_and_report("unreachable!()\n", &[(1, 0)]);

    let summary = summarize(&report, directory.path()).expect("report uncovered line");

    assert!(!summary.is_complete());
    assert_eq!(summary.excluded_lines, 0);
}

#[test]
fn covered_unreachable_lines_count_as_covered() {
    let (directory, report) = source_and_report("unreachable!(\"invariant\")\n", &[(1, 1)]);

    let summary = summarize(&report, directory.path()).expect("report covered line");

    assert!(summary.is_complete());
    assert_eq!(summary.excluded_lines, 0);
}

#[test]
fn does_not_exclude_unreachable_lines_missing_from_the_report() {
    let (directory, report) = source_and_report("unreachable!(\"invariant\")\n", &[]);

    let summary = summarize(&report, directory.path()).expect("summarize coverage");

    assert!(summary.is_complete());
    assert_eq!(summary.excluded_lines, 0);
}

#[test]
fn rejects_invalid_segment_data_for_unreachable_lines() {
    let (directory, report) = source_and_report("unreachable!(\"invariant\")\n", &[(1, 0)]);
    let report: Value = serde_json::from_str(&report).expect("parse fixture report");
    let report = report
        .as_object()
        .expect("report object")
        .get("data")
        .expect("data")
        .as_array()
        .expect("data array")[0]
        .get("files")
        .expect("files")
        .as_array()
        .expect("files array")[0]
        .as_object()
        .expect("file object")
        .clone();
    let malformed = json_with_files(json!([{
        "filename": report.get("filename").expect("filename"),
        "summary": report.get("summary").expect("summary"),
        "segments": [["invalid"]],
    }]));

    assert!(
        summarize(&malformed, directory.path())
            .expect_err("reject invalid segment")
            .to_string()
            .contains("invalid source segment")
    );
}

#[test]
fn rejects_invalid_segment_fields() {
    let (directory, _) = source_and_report("unreachable!(\"invariant\")\n", &[(1, 0)]);
    let source_path = directory.path().join("src/lib.rs");
    let segments = [
        (
            json!([null, 1, 0, true, true, false]),
            "invalid source line",
        ),
        (
            json!([1, 1, "invalid", true, true, false]),
            "invalid execution count",
        ),
        (
            json!([1, 1, 0, "invalid", true, false]),
            "no has-count value",
        ),
        (
            json!([1, 1, 0, true, true, "invalid"]),
            "no gap-region value",
        ),
    ];

    for (segment, expected_error) in segments {
        let report = report_for_path_with_data(&source_path, 1, 0, vec![segment]);
        assert!(
            summarize(&report, directory.path())
                .expect_err("invalid segment field should fail")
                .to_string()
                .contains(expected_error)
        );
    }
}

#[test]
fn reports_temporary_directory_creation_errors() {
    let directory = TemporaryDirectory::new();
    let file_path = directory.path().join("not-a-directory");
    fs::write(&file_path, "file").expect("create blocking file");

    assert!(
        CoverageReport::new_in(&file_path)
            .expect_err("file cannot contain a temporary directory")
            .to_string()
            .contains("failed to create temporary coverage report directory")
    );
}

#[test]
fn reports_missing_json_files() {
    let report = CoverageReport::new().expect("create temporary report directory");

    assert!(
        report
            .read(Path::new("."))
            .expect_err("missing report should fail")
            .to_string()
            .contains("failed to read coverage report")
    );
}
