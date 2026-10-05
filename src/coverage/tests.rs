// Released under the MIT License.
// Copyright, 2026, by Samuel Williams.

use super::{
    CoverageReport, Position, SourceRegion, compare_regions, find_unreachable_macros,
    resolve_source_path, summarize,
};
use crate::test_support::TemporaryDirectory;
use serde_json::{Value, json};
use std::fs;
use std::path::{Path, PathBuf};

fn source_path(directory: &TemporaryDirectory, source: &str) -> PathBuf {
    let path = directory.path().join("src/lib.rs");
    fs::create_dir_all(path.parent().expect("source parent")).expect("create source directory");
    fs::write(&path, source).expect("write source file");
    path
}

fn position(source: &str, offset: usize) -> (usize, usize) {
    let prefix = &source[..offset];
    let line = prefix.bytes().filter(|byte| *byte == b'\n').count() + 1;
    let column = offset - prefix.rfind('\n').map_or(0, |index| index + 1) + 1;
    (line, column)
}

fn region_for(source: &str, needle: &str, count: u64) -> Value {
    let start_offset = source.find(needle).expect("source contains region text");
    region_for_range(source, start_offset, start_offset + needle.len(), count)
}

fn region_for_range(source: &str, start: usize, end: usize, count: u64) -> Value {
    let (start_line, start_column) = position(source, start);
    let (end_line, end_column) = position(source, end);
    json!([
        start_line,
        start_column,
        end_line,
        end_column,
        count,
        0,
        0,
        0
    ])
}

fn function(path: &Path, regions: Vec<Value>) -> Value {
    json!({
        "name": "fixture",
        "filenames": [path],
        "regions": regions,
    })
}

fn report(path: &Path, functions: Vec<Value>) -> String {
    json!({
        "data": [{
            "files": [{"filename": path}],
            "functions": functions,
        }]
    })
    .to_string()
}

#[test]
fn merges_identical_source_regions_across_function_instantiations() {
    let directory = TemporaryDirectory::new();
    let source = "fn work() { call(); }\n";
    let path = source_path(&directory, source);
    let uncovered = region_for(source, "call()", 0);
    let covered = region_for(source, "call()", 2);
    let report = report(
        &path,
        vec![
            function(&path, vec![uncovered]),
            function(&path, vec![covered]),
        ],
    );

    let summary = summarize(&report, directory.path()).expect("summarize coverage");

    assert!(summary.is_complete());
    assert_eq!(summary.measured_regions, 1);
    assert_eq!(summary.covered_regions, 1);
}

#[test]
fn reuses_source_files_across_coverage_data_sets() {
    let directory = TemporaryDirectory::new();
    let source = "fn work() { run(); }\n";
    let path = source_path(&directory, source);
    let function = function(&path, vec![region_for(source, "run()", 1)]);
    let report = json!({
        "data": [
            {
                "files": [{"filename": path}],
                "functions": [function.clone()],
            },
            {
                "files": [{"filename": path}],
                "functions": [function],
            },
        ]
    })
    .to_string();

    let summary = summarize(&report, directory.path()).expect("summarize coverage");

    assert!(summary.is_complete());
    assert_eq!(summary.measured_regions, 1);
    assert_eq!(summary.covered_regions, 1);
}

#[test]
fn keeps_distinct_source_ranges_on_the_same_line() {
    let directory = TemporaryDirectory::new();
    let source = "fn work() { if ready { run(); } }\n";
    let path = source_path(&directory, source);
    let report = report(
        &path,
        vec![function(
            &path,
            vec![
                region_for(source, "ready", 1),
                region_for(source, "run()", 0),
            ],
        )],
    );

    let summary = summarize(&report, directory.path()).expect("summarize coverage");

    assert!(!summary.is_complete());
    assert_eq!(summary.measured_regions, 2);
    assert_eq!(summary.covered_regions, 1);
    assert_eq!(summary.uncovered_regions.len(), 1);
    let start = source.find("run()").expect("run call");
    let end = start + "run()".len();
    let (start_line, start_column) = position(source, start);
    let (end_line, end_column) = position(source, end);
    assert!(summary.failure_message().contains(&format!(
        "lib.rs:{start_line}:{start_column}-{end_line}:{end_column}"
    )));
}

#[test]
fn sorts_uncovered_source_locations() {
    let directory = TemporaryDirectory::new();
    let source = "fn work() { second(); first(); }\n";
    let path = source_path(&directory, source);
    let report = report(
        &path,
        vec![function(
            &path,
            vec![
                region_for(source, "second()", 0),
                region_for(source, "first()", 0),
            ],
        )],
    );

    let summary = summarize(&report, directory.path()).expect("summarize coverage");
    let message = summary.failure_message();
    let first_offset = source.find("first()").expect("first call");
    let second_offset = source.find("second()").expect("second call");
    let (_, first_column) = position(source, first_offset);
    let (_, second_column) = position(source, second_offset);
    let first_location = format!("lib.rs:1:{first_column}-");
    let second_location = format!("lib.rs:1:{second_column}-");
    let first = message
        .find(&first_location)
        .expect("first source location");
    let second = message
        .find(&second_location)
        .expect("second source location");

    assert!(second < first);
}

#[test]
fn compares_source_regions_by_each_location_field() {
    let region = SourceRegion {
        path: PathBuf::from("a.rs"),
        start: Position { line: 1, column: 1 },
        end: Position { line: 1, column: 2 },
        kind: 0,
    };
    let mut different = region.clone();

    different.path = PathBuf::from("b.rs");
    assert!(compare_regions(&region, &different).is_lt());
    different = region.clone();
    different.start.line = 2;
    assert!(compare_regions(&region, &different).is_lt());
    different = region.clone();
    different.start.column = 2;
    assert!(compare_regions(&region, &different).is_lt());
    different = region.clone();
    different.end.line = 2;
    assert!(compare_regions(&region, &different).is_lt());
    different = region.clone();
    different.end.column = 3;
    assert!(compare_regions(&region, &different).is_lt());
    different = region.clone();
    different.kind = 1;
    assert!(compare_regions(&region, &different).is_lt());
}

#[test]
fn excludes_regions_within_multiline_unreachable_calls() {
    let directory = TemporaryDirectory::new();
    let source =
        "fn work() {\n    unreachable!(\n        format!(\"reason )\", 1)\n    );\n    run();\n}\n";
    let path = source_path(&directory, source);
    let report = report(
        &path,
        vec![function(
            &path,
            vec![
                region_for(source, "unreachable!", 0),
                region_for(source, "format!", 0),
                region_for(source, "run()", 1),
            ],
        )],
    );

    let summary = summarize(&report, directory.path()).expect("summarize coverage");

    assert!(summary.is_complete());
    assert_eq!(summary.measured_regions, 1);
    assert_eq!(summary.excluded_unreachable_regions, 2);
}

#[test]
fn does_not_treat_macro_text_inside_strings_or_comments_as_unreachable() {
    let directory = TemporaryDirectory::new();
    let source = r##"let text = r#"unreachable!()"#; // unreachable!()
/* unreachable!() */
let unreachable = 1;
let unreachable_macro = 2;
run();
"##;
    let path = source_path(&directory, source);
    let report = report(
        &path,
        vec![function(&path, vec![region_for(source, "run()", 0)])],
    );

    let summary = summarize(&report, directory.path()).expect("summarize coverage");

    assert!(!summary.is_complete());
    assert_eq!(summary.excluded_unreachable_regions, 0);
    assert_eq!(summary.uncovered_regions.len(), 1);
}

#[test]
fn measures_regions_with_macro_names_and_delimiters() {
    let directory = TemporaryDirectory::new();
    let source = "fn work() { matches!(value, Some(_)); let result = call()?; }\n";
    let path = source_path(&directory, source);
    let report = report(
        &path,
        vec![function(
            &path,
            vec![
                region_for(source, "matches!", 0),
                region_for(source, "}", 0),
                region_for(source, "?", 0),
                region_for(source, "value", 1),
            ],
        )],
    );

    let summary = summarize(&report, directory.path()).expect("summarize coverage");

    assert!(!summary.is_complete());
    assert_eq!(summary.measured_regions, 4);
    assert_eq!(summary.covered_regions, 1);
    assert!(
        summary
            .failure_message()
            .contains("3 source regions remain uncovered")
    );
}

#[test]
fn ignores_non_executable_gap_and_skipped_regions() {
    let directory = TemporaryDirectory::new();
    let source = "fn work() { run(); }\n";
    let path = source_path(&directory, source);
    let mut regions = vec![region_for(source, "run()", 1)];
    let mut gap = region_for(source, "run()", 0);
    gap[7] = json!(3);
    let mut skipped = region_for(source, "run()", 0);
    skipped[7] = json!(2);
    regions.extend([gap, skipped]);
    let report = report(&path, vec![function(&path, regions)]);

    let summary = summarize(&report, directory.path()).expect("summarize coverage");

    assert!(summary.is_complete());
    assert_eq!(summary.measured_regions, 1);
}

#[test]
fn resolves_relative_source_paths() {
    let directory = TemporaryDirectory::new();
    let source = "fn work() { run(); }\n";
    let path = source_path(&directory, source);
    let report = report(
        &path,
        vec![function(&path, vec![region_for(source, "run()", 1)])],
    );
    let report = report.replace(&path.display().to_string(), "src/lib.rs");

    let summary = summarize(&report, directory.path()).expect("resolve relative source");

    assert!(summary.is_complete());
    assert_eq!(summary.measured_regions, 1);
}

#[test]
fn maps_source_regions_by_function_file_id() {
    let directory = TemporaryDirectory::new();
    let first_source = "fn first() { call(); }\n";
    let first_path = source_path(&directory, first_source);
    let second_path = directory.path().join("src/second.rs");
    fs::write(&second_path, "fn second() { call(); }\n").expect("write second source");
    let ignored_path = directory.path().join("src/ignored.rs");
    fs::write(&ignored_path, "fn ignored() { call(); }\n").expect("write ignored source");
    let mut second_region = region_for("fn second() { call(); }\n", "call()", 1);
    second_region[5] = json!(1);
    let mut ignored_region = region_for("fn ignored() { call(); }\n", "call()", 0);
    ignored_region[5] = json!(2);
    let function = json!({
        "name": "multiple-files",
        "filenames": [first_path, second_path, ignored_path],
        "regions": [region_for(first_source, "call()", 1), second_region, ignored_region],
    });
    let report = json!({
        "data": [{
            "files": [
                {"filename": directory.path().join("src/lib.rs")},
                {"filename": directory.path().join("src/second.rs")},
            ],
            "functions": [function],
        }]
    })
    .to_string();

    let summary = summarize(&report, directory.path()).expect("summarize multiple files");

    assert!(summary.is_complete());
    assert_eq!(summary.measured_regions, 2);
}

#[test]
fn finds_nested_and_braced_unreachable_macros() {
    let source = "fn work() { let x = r#\"unreachable!()\"#; std::unreachable! { outer({ [unreachable!()] }) }; }";
    let ranges = find_unreachable_macros(source);

    assert_eq!(ranges.len(), 1);
    assert_eq!(
        &source[ranges[0].clone()],
        "std::unreachable! { outer({ [unreachable!()] }) }"
    );
}

#[test]
fn finds_absolute_and_raw_identifier_unreachable_macros() {
    for source in ["::unreachable!()", "r#unreachable!()"] {
        let ranges = find_unreachable_macros(source);

        assert_eq!(ranges.len(), 1);
        assert_eq!(&source[ranges[0].clone()], source);
    }
}

#[test]
fn finds_square_bracket_unreachable_macros() {
    let source = "unreachable![call]";
    let ranges = find_unreachable_macros(source);

    assert_eq!(ranges.len(), 1);
    assert_eq!(&source[ranges[0].clone()], source);
}

#[test]
fn leaves_lifetime_syntax_as_code_while_finding_the_macro() {
    let source = "fn borrow<'a>() { unreachable!() }";
    let ranges = find_unreachable_macros(source);

    assert_eq!(ranges.len(), 1);
    assert_eq!(&source[ranges[0].clone()], "unreachable!()");
}

#[test]
fn skips_comments_and_string_and_character_literals() {
    let source = r####"
        let ordinary = "unreachable!() and \\\"still a string";
        let bytes = b"unreachable!()";
        let raw = r###"unreachable!()"###;
        let raw_bytes = br#"unreachable!()"#;
        let c_string = c"unreachable!()";
        let raw_c_string = cr#"unreachable!()"#;
        let character = 'u';
        let byte_character = b'\\';
        /* outer /* unreachable!() */ comment */
        // unreachable!()
        unreachable /* comment */ ! { call("}") }
    "####;
    let ranges = find_unreachable_macros(source);

    assert_eq!(ranges.len(), 1);
    assert_eq!(
        &source[ranges[0].clone()],
        "unreachable /* comment */ ! { call(\"}\") }"
    );
}

#[test]
fn resolves_paths_when_source_files_cannot_be_canonicalized() {
    let directory = TemporaryDirectory::new();
    assert_eq!(
        resolve_source_path(Path::new("missing.rs"), directory.path()),
        directory.path().join("missing.rs")
    );
}

#[test]
fn rejects_invalid_json_and_missing_report_sections() {
    let directory = TemporaryDirectory::new();
    assert!(
        summarize("{", directory.path())
            .expect_err("invalid JSON")
            .to_string()
            .contains("invalid cargo-llvm-cov JSON report")
    );
    assert!(
        summarize("{}", directory.path())
            .expect_err("missing data")
            .to_string()
            .contains("no data array")
    );
    assert!(
        summarize(&json!({"data": [{}]}).to_string(), directory.path())
            .expect_err("missing files")
            .to_string()
            .contains("no files array")
    );
    assert!(
        summarize(
            &json!({"data": [{"files": [], "functions": []}]}).to_string(),
            directory.path(),
        )
        .expect_err("empty file list")
        .to_string()
        .contains("contains no source files")
    );
    assert!(
        summarize(
            &json!({"data": [{"files": null, "functions": []}]}).to_string(),
            directory.path(),
        )
        .expect_err("files must be an array")
        .to_string()
        .contains("no files array")
    );
}

#[test]
fn rejects_source_file_and_function_metadata_errors() {
    let directory = TemporaryDirectory::new();
    assert!(
        summarize(
            &json!({"data": [{"files": [{}], "functions": []}]}).to_string(),
            directory.path(),
        )
        .expect_err("file requires filename")
        .to_string()
        .contains("no filename")
    );

    let path = source_path(&directory, "fn work() { run(); }\n");
    let invalid_functions = [
        (json!({"files": [{"filename": path}]}), "no functions array"),
        (
            json!({"files": [{"filename": path}], "functions": [{}]}),
            "no filenames array",
        ),
        (
            json!({
                "files": [{"filename": path}],
                "functions": [{"filenames": [path]}]
            }),
            "no regions array",
        ),
    ];

    for (item, expected) in invalid_functions {
        let report = json!({"data": [item]}).to_string();
        assert!(
            summarize(&report, directory.path())
                .expect_err("function metadata is incomplete")
                .to_string()
                .contains(expected)
        );
    }
}

#[test]
fn rejects_malformed_regions_and_invalid_file_ids() {
    let directory = TemporaryDirectory::new();
    let path = source_path(&directory, "fn work() { run(); }\n");
    let functions = [
        (json!([1, 1]), "invalid source region"),
        (
            json!([1, 1, 1, 2, 0, "invalid", 0, 0]),
            "source region file id is invalid",
        ),
        (
            json!([1, 1, 1, 2, "invalid", 0, 0, 0]),
            "execution count is invalid",
        ),
        (json!([1, 1, 1, 2, 0, 3, 0, 0]), "invalid file id"),
        (
            json!([1, 1, 1, 2, 0, 0, 0, "invalid"]),
            "source region kind is invalid",
        ),
        (
            json!(["invalid", 1, 1, 2, 0, 0, 0, 0]),
            "start line is invalid",
        ),
        (
            json!([1, "invalid", 1, 2, 0, 0, 0, 0]),
            "start column is invalid",
        ),
        (
            json!([1, 1, "invalid", 2, 0, 0, 0, 0]),
            "end line is invalid",
        ),
        (
            json!([1, 1, 1, "invalid", 0, 0, 0, 0]),
            "end column is invalid",
        ),
    ];

    for (region, expected) in functions {
        let report = report(&path, vec![function(&path, vec![region])]);
        assert!(
            summarize(&report, directory.path())
                .expect_err("malformed region")
                .to_string()
                .contains(expected)
        );
    }
}

#[test]
fn rejects_invalid_source_coordinates_and_region_order() {
    let directory = TemporaryDirectory::new();
    let source = "fn work() { run(); }\n";
    let path = source_path(&directory, source);
    let invalid_regions = [
        (json!([0, 1, 1, 2, 0, 0, 0, 0]), "zero source position"),
        (json!([99, 1, 99, 2, 0, 0, 0, 0]), "starts past the end"),
        (json!([1, 99, 1, 100, 0, 0, 0, 0]), "column exceeds"),
        (json!([1, 16, 1, 10, 0, 0, 0, 0]), "ends before it starts"),
        (json!([1, 1, 99, 1, 0, 0, 0, 0]), "starts past the end"),
        (
            json!([2, usize::MAX as u64, 2, 1, 0, 0, 0, 0]),
            "column exceeds",
        ),
    ];

    for (region, expected) in invalid_regions {
        let report = report(&path, vec![function(&path, vec![region])]);
        assert!(
            summarize(&report, directory.path())
                .expect_err("invalid region coordinates")
                .to_string()
                .contains(expected)
        );
    }
}

#[test]
fn rejects_source_regions_that_split_utf8_characters() {
    let directory = TemporaryDirectory::new();
    let source = "fn work() { let value = 'é'; }\n";
    let path = source_path(&directory, source);
    let character_offset = source
        .find('é')
        .expect("source contains a Unicode character");
    let column = character_offset
        - source[..character_offset]
            .rfind('\n')
            .map_or(0, |index| index + 1)
        + 1;
    let region = json!([1, column + 1, 1, column + 2, 0, 0, 0, 0]);
    let report = report(&path, vec![function(&path, vec![region])]);

    assert!(
        summarize(&report, directory.path())
            .expect_err("a byte offset inside a Unicode character should fail")
            .to_string()
            .contains("does not align with source text")
    );
}

#[test]
fn rejects_coverage_reports_without_function_regions() {
    let directory = TemporaryDirectory::new();
    let path = source_path(&directory, "fn work() {}\n");
    let report = report(&path, vec![function(&path, vec![])]);

    assert!(
        summarize(&report, directory.path())
            .expect_err("report has no regions")
            .to_string()
            .contains("contains no source regions")
    );
}

#[test]
fn reports_covered_source_read_errors() {
    let directory = TemporaryDirectory::new();
    let path = directory.path().join("missing.rs");
    let report = report(&path, vec![]);

    assert!(
        summarize(&report, directory.path())
            .expect_err("missing source")
            .to_string()
            .contains("failed to read covered source file")
    );
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
