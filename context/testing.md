# Rust Testing Tasks

Add `bake-test-rust` as a dependency of the private `bake/` package and link it
from `bake/src/main.rs`:

```toml
[dependencies]
bake-test-rust = "0.1"
```

```rust,ignore
use bake_test_rust as _;
```

The package registers `test` and `test:external` tasks. `test` runs
`cargo test --workspace --locked`; `test:external` runs Cargo tests in the
selected downstream repositories without `--locked`, because the lockfiles
need to resolve local path patches.

## Before-test hook

Both tasks call the optional project task `test:before` once before running
tests. Use it for resources that need to be prepared for either local or
downstream tests, such as downloading fixtures or generating assets:

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

The hook is optional. If it is not registered, the test tasks continue without
calling it. External checkout and patch preparation happens before the hook;
the hook runs once before Cargo tests begin. If no downstream repositories are
configured, `test:external` returns a message without running the hook.

## External repositories

List selected downstream repositories in the root `Cargo.toml`:

```toml
[[workspace.metadata.bake.test.external]]
repository = "https://github.com/socketry/downstream-project"
branch = "main"
```

For a single-package project, use
`[[package.metadata.bake.test.external]]`. `branch` defaults to `main`. Add
`name = "checkout-name"` when you want the local checkout directory to use a
specific name or avoid a name collision. If the list is empty or absent,
`test:external` reports that no downstream repositories are configured.

The task clones each repository into `external/<name>/` on first use. It keeps
existing Git checkouts and does not fetch, reset, switch branches, or discard
local edits. To update a checkout, run Git commands inside it. If the metadata
repository no longer matches the checkout, choose a new `name` or move the old
directory yourself.

The task adds this workspace's crates.io-publishable packages to the cloned
repository's `[patch.crates-io]` table, using relative paths back to the local
workspace. Existing unrelated manifest content is preserved, and the same
patches are not inserted a second time. If a conflicting patch already exists,
the task stops and asks you to resolve that entry. The patch remains in the
checkout so you can enter it and run `cargo test --workspace` manually to
investigate a failure. Add `/external/` to the consumer repository's
`.gitignore`, as this repository does.

Cargo still checks the downstream dependency's version requirement. If the
local workspace version does not satisfy it, update that requirement in the
checkout before rerunning external tests. The task checks Cargo's resolved
dependency graph and stops if a downstream dependency silently resolves to the
registry version instead of the local patch.

## GitHub Actions

Keep `.github/workflows/test.yml` as the standard local test workflow and run
`cargo bake --locked test` so the `test:before` hook also runs in CI. Add
`.github/workflows/external.yml` only when the Cargo metadata list contains
one or more repositories:

```yaml
name: External Tests

on:
  push:
  pull_request:
  workflow_dispatch:

permissions:
  contents: read

jobs:
  test:
    runs-on: ubuntu-latest
    timeout-minutes: 30
    steps:
      - uses: actions/checkout@v7
      - uses: dtolnay/rust-toolchain@stable
      - name: Install Bake launcher
        run: cargo install socketry-cargo-bake --locked
      - name: Run downstream compatibility tests
        run: cargo bake --locked test:external
```

The external task creates fresh checkouts in the workflow's temporary runner
and applies the local patches before testing. Remove this workflow when the
metadata list becomes empty.
