# `bake-test-rust`

Reusable Bake tasks for running Rust tests and testing downstream repositories
against local Cargo workspace packages.

## Motivation

Rust provides a good test runner through Cargo, but it does not provide a shared
way to prepare project resources or exercise selected downstream crates. This
package gives Rust projects a consistent `bake test` and `bake test:external`
interface while keeping Cargo as the test runner.

## Usage

Add the task crate to the private `bake/` package:

```toml
[dependencies]
bake-test-rust = "0.1"
```

Link its tasks from `bake/src/main.rs`:

```rust,ignore
use bake_test_rust as _;
```

Then run the workspace tests:

```sh
cargo bake test
cargo bake test:external
```

Configure downstream repositories in the root `Cargo.toml`:

```toml
[[workspace.metadata.bake.test.external]]
repository = "https://github.com/socketry/downstream-project"
branch = "main"
```

See the [Rust Testing guide](context/testing.md) for the `test:before` hook,
checkout reuse, Cargo patching, and GitHub Actions setup.

### Agent Context

This crate publishes context files for coding agents. Configure Bake Agent
Context in your private `bake/` crate, then run
`cargo bake agent:context:install --package bake-test-rust` to install them in
`.agents/context/` and update `agents.md`.

## Releasing

Prepare a release with `cargo bake cargo:version:patch` (or `minor`, `major`,
or `bump --version X.Y.Z`), then run `cargo bake cargo:release` and open a
pull request. After review and merge, GitHub Actions publishes the release
when the configured `crates-io` environment approves it. See the
[Cargo publishing guide](https://github.com/socketry/bake-cargo-rust/blob/main/context/publishing.md).

## Releases

<!-- bake-readme:releases:start -->
See [releases.md](releases.md) for the full release history.

### v0.1.0

- Add standard Rust test and downstream compatibility tasks for Bake.

## See Also

- [Bake](https://github.com/socketry/bake-rust)
- [Bake Cargo](https://github.com/socketry/bake-cargo-rust)

## Contributing

Please open an issue or pull request on [GitHub](https://github.com/socketry/bake-test-rust).
