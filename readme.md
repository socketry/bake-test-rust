# `bake-test-rust`

Reusable Bake tasks for running Rust tests and testing downstream repositories against local Cargo workspace packages.

## Motivation

Rust provides a good test runner through Cargo, but it does not provide a shared way to prepare project resources or exercise selected downstream crates. This package gives Rust projects a consistent `bake test`, `bake test:coverage`, and `bake test:external` interface while keeping Cargo and cargo-llvm-cov as the test runners.

## Usage

Add the task crate to the private `bake/` package:

```sh
cargo bake --regenerate
cargo add --manifest-path bake/Cargo.toml bake-test-rust
cargo bake --regenerate
```

Then run the workspace tests:

```sh
cargo bake test
cargo bake test:coverage
cargo bake test:external
```

The coverage task runs documentation tests and requires 100% measured source-region coverage for the selected workspace test configuration. Regions inside `unreachable!()` calls are excluded automatically. It reports uncovered source locations when the gate fails. Install `cargo-llvm-cov` first; see the [Rust Testing guide](context/testing.md) for feature selection and architecture notes. Use `--package name` to limit a workspace run to one package.

Configure downstream repositories in the root `Cargo.toml`:

```toml
[[workspace.metadata.bake.test.external]]
repository = "https://github.com/socketry/downstream-project"
branch = "main"
```

See the [Rust Testing guide](context/testing.md) for the `test:before` hook, checkout reuse, Cargo patching, and canonical GitHub Actions workflows.

## Releasing

Prepare a release with `cargo bake cargo:version:patch` (or `minor`, `major`, or `bump --version X.Y.Z`), then run `cargo bake cargo:release` and open a pull request. After review and merge, GitHub Actions publishes the release when the configured `crates-io` environment approves it. Follow the shared [Releasing skill](https://github.com/socketry/socketry-project-rust/blob/main/context/releasing.md) for the standard process.

## Releases

<!-- bake-readme:releases:start -->

See [releases.md](releases.md) for the full release history.

### v0.3.2

- Use Cargo-selected dependency versions and generated task links in setup instructions.

### v0.3.1

- Adopt `socketry-project` 0.3.7 for shared project tasks and Markdown normalization.
- Require the aggregate test and coverage result for pull request merges.
- Document the canonical aggregate test gate and current coverage dependency.

### v0.3.0

- Enforce complete source-region coverage, excluding regions inside invariant-only `unreachable!()` calls.

<!-- bake-readme:releases:end -->

## See Also

- [Bake](https://github.com/socketry/bake-rust)
- [Bake Cargo](https://github.com/socketry/bake-cargo-rust)

## Contributing

Please open an issue or pull request on [GitHub](https://github.com/socketry/bake-test-rust).

### Agent Context

Run `cargo bake agent:context:install` to install shared context and skills. Read `.agents/context/index.md` to find relevant guides, follow `agents.md` if present, and apply skills under `.agents/skills/`. The installer preserves repository-owned `agents.md`; it does not create or regenerate that file.
