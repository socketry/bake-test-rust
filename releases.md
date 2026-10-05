# Releases

## v0.3.2

- Use Cargo-selected dependency versions and generated task links in setup instructions.

## v0.3.1

- Adopt `socketry-project` 0.3.7 for shared project tasks and Markdown normalization.
- Require the aggregate test and coverage result for pull request merges.
- Document the canonical aggregate test gate and current coverage dependency.

## v0.3.0

- Enforce complete source-region coverage, excluding regions inside invariant-only `unreachable!()` calls.

## v0.2.3

- Exclude uncovered, single-line `unreachable!("reason")` expressions from measured coverage automatically.

## v0.2.2

- Declare compatibility with the Bake 0.x API so task libraries can share one task registry when upgrading to crate-derived task namespaces.

- Document how to install and discover dependency context and skills.

- Assert external test runs refresh each checkout's Cargo lockfile.

## v0.2.1

- Use the shared `socketry-project` Releasing skill for the standard release process and remove references to the duplicate Bake Cargo publishing context.

## v0.2.0

- Add a Bake task for 100% line coverage with missing-line output.

## v0.1.2

- Update downstream lockfiles after applying local workspace patches.

## v0.1.1

- Fix the Bake task name so `cargo bake test` invokes the workspace test task.

## v0.1.0

- Add standard Rust test and downstream compatibility tasks for Bake.
