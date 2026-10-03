# `bake-test-rust`

Reusable Bake tasks for running Rust tests and testing downstream repositories
against local Cargo workspace packages.

## Motivation

Rust provides a good test runner through Cargo, but it does not provide a shared
way to prepare project resources or exercise selected downstream crates. This
package gives Rust projects a consistent `bake test`, `bake test:coverage`,
and `bake test:external` interface while keeping Cargo and cargo-llvm-cov as
the test runners.

## Usage

Add the task crate to the private `bake/` package:

```toml
[dependencies]
bake-test-rust = "0.2"
```

Link its tasks from `bake/src/main.rs`:

```rust,ignore
use bake_test_rust as _;
```

Then run the workspace tests:

```sh
cargo bake test
cargo bake test:coverage
cargo bake test:external
```

The coverage task runs documentation tests and requires 100% line coverage for
the selected workspace test configuration. It prints uncovered source lines
when the gate fails. Install `cargo-llvm-cov` first; see the
[Rust Testing guide](context/testing.md) for feature selection and architecture
notes. Use `--package name` to limit a workspace run to one package.

Configure downstream repositories in the root `Cargo.toml`:

```toml
[[workspace.metadata.bake.test.external]]
repository = "https://github.com/socketry/downstream-project"
branch = "main"
```

See the [Rust Testing guide](context/testing.md) for the `test:before` hook,
checkout reuse, Cargo patching, and canonical GitHub Actions workflows.

### Agent Context

This crate publishes context files for coding agents. Configure Bake Agent
Context in your private `bake/` crate, then run
`cargo bake agent:context:install --package bake-test-rust` to install the
package's testing guidance for agents. See the Bake Agent Context guide for
installation details.

## Releasing

Prepare a release with `cargo bake cargo:version:patch` (or `minor`, `major`,
or `bump --version X.Y.Z`), then run `cargo bake cargo:release` and open a
pull request. After review and merge, GitHub Actions publishes the release
when the configured `crates-io` environment approves it. Follow the shared
[Releasing skill](https://github.com/socketry/socketry-project-rust/blob/main/context/releasing.md)
for the standard process.

## Releases

<!-- bake-readme:releases:start -->
See [releases.md](releases.md) for the full release history.

### v0.2.1

- Use the shared `socketry-project` Releasing skill for the standard release
  process and remove references to the duplicate Bake Cargo publishing context.

### v0.2.0

- Add a Bake task for 100% line coverage with missing-line output.

### v0.1.2

- Update downstream lockfiles after applying local workspace patches.
<!-- bake-readme:releases:end -->

## See Also

- [bake-test-rust](https://github.com/socketry/bake-test-rust) — Reusable Rust test tasks for Bake <!-- bake-readme:package -->

- [Bake](https://github.com/socketry/bake-rust)
- [Bake Cargo](https://github.com/socketry/bake-cargo-rust)

## Contributing

Please open an issue or pull request on [GitHub](https://github.com/socketry/bake-test-rust).
