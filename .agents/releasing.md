Use `cargo bake cargo:version:patch` (or `minor`, `major`, or
`bump --version X.Y.Z`) to prepare a release. Review the generated changes,
then run `cargo bake cargo:release` and open a pull request. The merged release
pull request is published by GitHub Actions after the configured `crates-io`
environment approves it.
