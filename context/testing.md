# Rust Testing Tasks

Add `bake-test-rust` as a dependency of the private `bake/` package and regenerate its task links:

```sh
cargo bake --regenerate
cargo add --manifest-path bake/Cargo.toml bake-test-rust
cargo bake --regenerate
```

The package registers `test`, `test:coverage`, and `test:external` tasks. `test` runs `cargo test --workspace --locked`; `test:external` runs Cargo tests in the selected downstream repositories without `--locked`, because the lockfiles need to resolve local path patches.

## Coverage

Install `cargo-llvm-cov` and run the Bake coverage task:

```sh
rustup component add llvm-tools-preview --toolchain stable
cargo +stable install cargo-llvm-cov --locked
rustup run stable cargo bake test:coverage
```

Run the task with the same toolchain that has `llvm-tools-preview`. If another Rust installation such as Homebrew's `cargo` comes first on `PATH`, plain `cargo bake` may use that compiler and fail to find the Rustup component.

The task calls the optional `test:before` hook once, runs documentation tests with Cargo, then runs workspace tests under `cargo-llvm-cov`. It reports each uncovered source region and fails if any measured region remains uncovered. Regions with identical source spans are merged across function instantiations; distinct source spans remain separate, including spans on the same line. The task covers the whole workspace by default; use `--package name` to limit coverage to one package. Coverage uses the default feature set unless `--all-features true` or one or more repeatable `--features name` arguments are supplied. Choose one feature configuration per invocation; the task rejects combining `--all-features` and `--features`.

### Invariant-only `unreachable!()` calls

Rust checks local type-level impossibilities, such as exhaustive matches over closed enums. It does not generally prove runtime invariants that depend on relationships between values, parser behavior, or upstream code. An `unreachable!()` is a runtime assertion that panics if that invariant is violated; it is not proof that the branch cannot occur.

Regions inside an `unreachable!()` invocation are excluded from coverage, including multiline invocations. This exclusion is automatic and does not need a marker. It applies only to Rust source regions contained within the macro call; other uncovered code on the same line remains measured. The source scan ignores strings and comments.

Coverage uses the source regions reported by LLVM without classifying regions by their source text. A reported region remains measured even if its span contains only a delimiter or a macro name: LLVM can map executable behavior to such a span, including a branch outcome mapped to a closing brace. Inspect an uncovered region and the behavior represented by its mapping; do not exclude it solely because its span looks syntactic.

If `cargo-llvm-cov` reports a genuinely invariant-only `unreachable!()` call, include its reason in the panic message:

```rust
_ => unreachable!("Only JSX events can be mismatched here")
```

The macro asserts that the path should not be reached; it does not prove that the invariant is correct. Use `unreachable!()` only after checking the invariant against supported inputs. Test valid inputs and the boundary that establishes the invariant; do not manufacture an impossible private state solely to execute the panic. All other source-backed regions must reach 100% coverage.

The canonical GitHub Actions workflow below installs the required Rust components and coverage tool. The task measures the runner's target and feature configuration. If a crate contains architecture-specific code, run the same gate on each supported architecture; each run checks the code compiled for its target.

The coverage task runs documentation tests but does not include them in the coverage report; LLVM doctest coverage is still unstable. It uses the JSON function-region data to enforce 100% of measured source regions. This task measures source regions, not LLVM's separate experimental branch-coverage metric.

## Before-test hook

The `test`, `test:coverage`, and `test:external` tasks call the optional project task `test:before` once before running tests. Use it for resources that need to be prepared for either local or downstream tests, such as downloading fixtures or generating assets:

```rust,ignore
#[bake::task(name = "test:before")]
fn before_test(context: &mut bake::Context) -> bake::Result<()> {
    let status = context.command("make").arg("test-assets").status()?;
    if status.success() {
        Ok(())
    } else {
        Err(bake::Error::new(format!("test asset preparation failed: {status}")))
    }
}
```

The hook is optional. If it is not registered, the test tasks continue without calling it. External checkout and patch preparation happens before the hook; the hook runs once before Cargo tests begin. If no downstream repositories are configured, `test:external` returns a message without running the hook.

## External repositories

List selected downstream repositories in the root `Cargo.toml`:

```toml
[[workspace.metadata.bake.test.external]]
repository = "https://github.com/socketry/downstream-project"
branch = "main"
```

For a single-package project, use `[[package.metadata.bake.test.external]]`. `branch` defaults to `main`. Add `name = "checkout-name"` when you want the local checkout directory to use a specific name or avoid a name collision. If the list is empty or absent, `test:external` reports that no downstream repositories are configured.

The task clones each repository into `external/<name>/` on first use. It keeps existing Git checkouts and does not fetch, reset, switch branches, or discard local edits. To update a checkout, run Git commands inside it. If the metadata repository no longer matches the checkout, choose a new `name` or move the old directory yourself.

The task adds this workspace's crates.io-publishable packages to the cloned repository's `[patch.crates-io]` table, using relative paths back to the local workspace. Existing unrelated manifest content is preserved, and the same patches are not inserted a second time. If a conflicting patch already exists, the task stops and asks you to resolve that entry. The patch remains in the checkout so you can enter it and run `cargo test --workspace` manually to investigate a failure. Add `/external/` to the consumer repository's `.gitignore`, as this repository does.

Cargo still checks the downstream dependency's version requirement. If the local workspace version does not satisfy it, update that requirement in the checkout before rerunning external tests. The task checks Cargo's resolved dependency graph and stops if a downstream dependency silently resolves to the registry version instead of the local patch.

## Canonical GitHub workflows

Use this `test.yml` for Socketry Rust repositories. It runs formatting, Clippy, documentation tests, and workspace coverage. The coverage task invokes the optional `test:before` hook and requires 100% coverage of measured source regions. Passing `--all-targets true` includes examples and benchmarks in the coverage run. Do not add a separate `cargo bake test` step to this job; the coverage task runs the tests itself.

Filter both `push` and `pull_request` to `main`. Pull request updates then run through `pull_request`, without a duplicate run from the feature-branch push; merging to `main` still runs the workflow after the merge. Keep `workflow_dispatch` for manual runs.

```yaml
name: Test

on:
  push:
    branches: [main]
  pull_request:
    branches: [main]
  workflow_dispatch:

permissions:
  contents: read

jobs:
  test:
    runs-on: ubuntu-latest
    timeout-minutes: 20
    steps:
      - uses: actions/checkout@v7
      - uses: actions-rust-lang/setup-rust-toolchain@v2
        with:
          components: clippy, llvm-tools-preview, rustfmt
          # Isolate this source-installed tool from the old prebuilt-action cache.
          cache-shared-key: coverage-cargo-install-v1
      - name: Install Bake launcher
        run: cargo install socketry-cargo-bake --locked
      - name: Install coverage tool
        run: cargo install cargo-llvm-cov --locked
      - run: cargo fmt --all -- --check
      - run: cargo clippy --workspace --all-targets --locked -- -D warnings
      - name: Run tests and require complete source-region coverage
        run: cargo bake --locked test:coverage --all-targets true

  test-result:
    if: always()
    needs: [test]
    runs-on: ubuntu-latest
    timeout-minutes: 5
    steps:
      - name: Require successful tests and coverage
        env:
          JOB_RESULTS: ${{ toJSON(needs) }}
        run: |
          echo "$JOB_RESULTS" | jq -e 'all(.[]; .result == "success")'
```

Require `test-result` alongside the publishing workflow's `check` job in the main-branch ruleset. For a matrix or several required test jobs, list every required job in `needs`. The aggregate runs even when a prerequisite fails, and succeeds only when every listed job succeeds. Keep experimental or diagnostic jobs outside that required gate.

The standard `test` task remains useful for quick local runs without a coverage report. CI uses `test:coverage` so it enforces the organization-wide source-region coverage requirement.

External compatibility testing is optional. Add `.github/workflows/external.yml` only when the Cargo metadata list contains one or more selected downstream repositories:

Use the same event filters as `test.yml`: pull requests targeting `main` and pushes to `main`. This prevents duplicate downstream test runs for a pull request branch while still checking the merged default branch.

```yaml
name: External Tests

on:
  push:
    branches: [main]
  pull_request:
    branches: [main]
  workflow_dispatch:

permissions:
  contents: read

jobs:
  test:
    runs-on: ubuntu-latest
    timeout-minutes: 30
    steps:
      - uses: actions/checkout@v7
      - uses: actions-rust-lang/setup-rust-toolchain@v2
      - name: Install Bake launcher
        run: cargo install socketry-cargo-bake --locked
      - name: Run downstream compatibility tests
        run: cargo bake --locked test:external
```

The external task creates fresh checkouts in the workflow's temporary runner and applies the local patches before testing. Remove this workflow when the metadata list becomes empty.
