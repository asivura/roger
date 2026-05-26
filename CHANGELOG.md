# Changelog

All notable changes to this project are documented here. Format follows
[Keep a Changelog](https://keepachangelog.com/en/1.1.0/); the project
follows [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Added

- Initial Zellij plugin scaffold targeting `zellij-tile 0.43.1` for
  the `wasm32-wasip1` target. Plugin hides itself on load, requests
  the permissions the eventual RPC layer needs, subscribes to pane
  lifecycle events, and stubs the `pipe()` entrypoint where the shim
  CLI will deliver `team.spawn` / `team.send` / `team.kill` /
  `team.list` calls. ([#17](https://github.com/asivura/roger/pull/17))
- `ROADMAP.md` capturing the project vision, two-crate architecture,
  six implementation phases, and versioning + release policy.
  ([#15](https://github.com/asivura/roger/pull/15))
- GitHub Actions CI: `cargo fmt --check`, clippy with `-D warnings`,
  release build to `wasm32-wasip1`, Wasm artifact upload, and PR
  commit-message linting against Conventional Commits via
  `wagoid/commitlint-github-action`. ([#17](https://github.com/asivura/roger/pull/17))
- `.github/dependabot.yml` for `github-actions` and `cargo`,
  weekly cadence, with `ci` / `build` commit-prefix conventions so
  Dependabot PRs pass the commitlint gate. ([#21](https://github.com/asivura/roger/pull/21))
- `rust-toolchain.toml` with `channel = "stable"` and
  `targets = ["wasm32-wasip1"]`, so `cargo` auto-installs the right
  toolchain on first invocation. ([#26](https://github.com/asivura/roger/pull/26))
- `.cargo/config.toml` aliases `cargo build-wasm` and
  `cargo check-all` matching the CI invocations exactly.
  ([#26](https://github.com/asivura/roger/pull/26))
- `CONTRIBUTING.md`, `RELEASING.md`, `CHANGELOG.md`, and a PR
  template documenting the project conventions.
  ([#27](https://github.com/asivura/roger/pull/27))
- Cargo workspace restructure: code is now split across `plugin/`
  (package `roger`, the Wasm crate) and `shim/` (package
  `roger-shim`, a host-target binary named `tmux` that will translate
  Claude Code's TmuxBackend invocations into roger RPC calls in
  Phase C). Shim is a v0 stub today. (this PR)
- `cargo build-shim` / `cargo check-shim` aliases for the host-target
  shim crate. (this PR)

### Changed

- Hardened CI: third-party actions SHA-pinned, runners pinned to
  `ubuntu-24.04`, `timeout-minutes` set on both jobs, Wasm artifact
  upload gated to `main` pushes only.
  ([#21](https://github.com/asivura/roger/pull/21))
- Clippy invocation switched from `--all-targets` to
  `--target wasm32-wasip1 --all-features --lib --no-deps`, making the
  workflow workspace-ready for the upcoming `plugin/` + `shim/`
  split. ([#25](https://github.com/asivura/roger/pull/25))
- `[profile.release]` now sets `panic = "abort"` and
  `codegen-units = 1`. Measured ~2.6% Wasm size reduction on the v0
  scaffold; ratio grows as code volume increases.
  ([#25](https://github.com/asivura/roger/pull/25))
- `.commitlintrc.yml` disables `body-max-line-length` and
  `footer-max-line-length` so contributors can paste logs or diffs
  into commit bodies. Subject is still capped at 72.
  ([#26](https://github.com/asivura/roger/pull/26))

### Fixed

- `PermissionType::ReadPaneContents` removed from the plugin's
  initial permission request; it was added in `zellij-tile 0.44`
  but the scaffold targets 0.43.1, so the request was a compile
  error left over from an incomplete prior fix.
  ([#16](https://github.com/asivura/roger/pull/16))

[Unreleased]: https://github.com/asivura/roger/compare/HEAD...main
