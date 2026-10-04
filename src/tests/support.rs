// Released under the MIT License.
// Copyright, 2026, by Samuel Williams.

use std::ffi::{OsStr, OsString};
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Mutex, MutexGuard};
use std::time::{SystemTime, UNIX_EPOCH};

static ENVIRONMENT_LOCK: Mutex<()> = Mutex::new(());
static TEMPORARY_DIRECTORY_ID: AtomicU64 = AtomicU64::new(0);

pub struct TemporaryDirectory(PathBuf);

impl TemporaryDirectory {
    pub fn new() -> Self {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system clock is after the Unix epoch")
            .as_nanos();
        let identifier = TEMPORARY_DIRECTORY_ID.fetch_add(1, Ordering::Relaxed);
        let path = std::env::temp_dir().join(format!(
            "bake-test-rust-{}-{unique}-{identifier}",
            std::process::id()
        ));
        fs::create_dir(&path).expect("create temporary test directory");
        Self(path)
    }

    pub fn path(&self) -> &Path {
        &self.0
    }
}

impl Drop for TemporaryDirectory {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

pub struct Environment {
    _lock: MutexGuard<'static, ()>,
    original: Vec<(OsString, Option<OsString>)>,
}

impl Environment {
    pub fn new() -> Self {
        let lock = ENVIRONMENT_LOCK
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let keys = [
            "CARGO",
            "PATH",
            "BAKE_TEST_CARGO_LOG",
            "BAKE_TEST_COVERAGE_REPORT",
            "BAKE_TEST_FAIL_ON",
            "BAKE_TEST_METADATA_EXIT",
            "BAKE_TEST_WORKSPACE_METADATA",
            "BAKE_TEST_DOWNSTREAM_METADATA",
            "BAKE_TEST_GIT_LOG",
            "BAKE_TEST_GIT_FAIL",
            "BAKE_TEST_GIT_ORIGIN",
            "BAKE_TEST_GIT_NO_ORIGIN",
        ];
        let original = keys
            .iter()
            .map(|key| {
                let key: OsString = (*key).into();
                (key.clone(), std::env::var_os(&key))
            })
            .collect();

        Self {
            _lock: lock,
            original,
        }
    }

    pub fn set(&self, key: &str, value: impl AsRef<OsStr>) {
        // All tests that change process-wide environment variables hold this lock.
        unsafe { std::env::set_var(key, value) };
    }

    pub fn remove(&self, key: &str) {
        // All tests that change process-wide environment variables hold this lock.
        unsafe { std::env::remove_var(key) };
    }

    pub fn prepend_path(&self, directory: &Path) {
        let path = std::env::var_os("PATH").unwrap_or_default();
        let mut paths = vec![directory.to_owned()];
        paths.extend(std::env::split_paths(&path));
        let path = std::env::join_paths(paths).expect("join test PATH entries");
        self.set("PATH", path);
    }
}

impl Drop for Environment {
    fn drop(&mut self) {
        for (key, value) in &self.original {
            // All tests that change process-wide environment variables hold this lock.
            unsafe {
                if let Some(value) = value {
                    std::env::set_var(key, value);
                } else {
                    std::env::remove_var(key);
                }
            }
        }
    }
}

#[cfg(unix)]
pub fn install_fake_cargo(directory: &Path) -> PathBuf {
    let executable = directory.join("cargo-test-double");
    fs::write(
        &executable,
        r##"#!/bin/sh
printf '%s\n' "$*" >> "$BAKE_TEST_CARGO_LOG"
if [ "$1" = "metadata" ]; then
	if [ "$4" = "--no-deps" ]; then
		cat "$BAKE_TEST_WORKSPACE_METADATA"
	else
		cat "$BAKE_TEST_DOWNSTREAM_METADATA"
	fi
	exit "${BAKE_TEST_METADATA_EXIT:-0}"
fi
if [ -n "$BAKE_TEST_FAIL_ON" ] && [ "$1" = "$BAKE_TEST_FAIL_ON" ]; then
	echo "fake cargo failure" >&2
	exit 7
fi
if [ "$1" = "llvm-cov" ]; then
	shift
	while [ "$#" -gt 1 ]; do
		if [ "$1" = "--output-path" ]; then
			cp "$BAKE_TEST_COVERAGE_REPORT" "$2"
			exit $?
		fi
		shift
	done
	echo "fake llvm-cov invocation did not include --output-path" >&2
	exit 12
fi
exit 0
"##,
    )
    .expect("write fake Cargo executable");
    make_executable(&executable);
    executable
}

#[cfg(unix)]
pub fn install_fake_git(directory: &Path) -> PathBuf {
    let executable = directory.join("git");
    fs::write(
        &executable,
        r##"#!/bin/sh
printf '%s\n' "$*" >> "$BAKE_TEST_GIT_LOG"
if [ "$1" = "clone" ]; then
	if [ "$BAKE_TEST_GIT_FAIL" = "true" ]; then
		echo "fake git clone failure" >&2
		exit 8
	fi
	destination="$5"
	mkdir -p "$destination/.git" || exit 9
	printf '[workspace]\nmembers = []\n' > "$destination/Cargo.toml"
	exit 0
fi
if [ "$1" = "-C" ]; then
	if [ "$BAKE_TEST_GIT_NO_ORIGIN" = "true" ]; then
		echo "fake origin failure" >&2
		exit 10
	fi
	printf '%s\n' "$BAKE_TEST_GIT_ORIGIN"
	exit 0
fi
exit 11
"##,
    )
    .expect("write fake Git executable");
    make_executable(&executable);
    executable
}

#[cfg(unix)]
fn make_executable(path: &Path) {
    use std::os::unix::fs::PermissionsExt;

    let mut permissions = fs::metadata(path)
        .expect("read executable metadata")
        .permissions();
    permissions.set_mode(0o755);
    fs::set_permissions(path, permissions).expect("make test command executable");
}
