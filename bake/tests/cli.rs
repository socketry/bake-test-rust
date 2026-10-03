// Released under the MIT License.
// Copyright, 2026, by Samuel Williams.

use std::process::Command;

#[test]
fn executable_lists_discovered_tasks() {
    let output = Command::new(env!("CARGO_BIN_EXE_bake-test-rust-project"))
        .arg("--list")
        .output()
        .expect("run Bake task executable");

    assert!(output.status.success());
    let output = String::from_utf8(output.stdout).expect("task listing is UTF-8");
    assert!(output.contains("agent:context:install"));
    assert!(output.contains("cargo:release"));
    assert!(output.contains("test:coverage"));
    assert!(output.contains("test:external"));
    assert!(output.contains("license:update"));
    assert!(output.contains("readme:update"));
    assert!(output.contains("releases:update"));
    assert!(output.contains("cargo:after_version_bump"));
}
