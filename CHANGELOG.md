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
  Phase C). Shim is a v0 stub today.
  ([#28](https://github.com/asivura/roger/pull/28))
- `cargo build-shim` / `cargo check-shim` aliases for the host-target
  shim crate.
  ([#28](https://github.com/asivura/roger/pull/28))
- Mandatory multi-angle PR review workflow documented in `CLAUDE.md`:
  every PR Claude creates here gets a team of 5-10 AI reviewer agents
  whose angles are chosen based on PR context. Each posts a PR comment;
  the lead synthesizes them into one consolidation comment.
  ([#29](https://github.com/asivura/roger/pull/29))
- JSON-RPC-style protocol over `zellij pipe`, documented in
  `docs/rpc-protocol.md`, with `team.list` as the first method
  implemented. `team.list` returns currently-tracked teammate panes
  (empty until `team.spawn` populates the state map in #6).
  ([#35](https://github.com/asivura/roger/pull/35))
- New `roger-proto` crate containing the wire types
  (`Request`/`Response`/`ErrorPayload`/`TeammatePaneInfo`/`TeamListResult`/`error_codes`),
  separated from the plugin crate so they can be unit-tested on the
  host target without the `zellij-tile` link error. 10 unit tests
  covering serde round-trip, the exactly-one-of `result`/`error`
  invariant, JSON-RPC 2.0 reserved-range codes, and optional-field
  omission. CI now runs `cargo test -p roger-proto` on every push
  and PR.
  ([#40](https://github.com/asivura/roger/pull/40))
- `cargo test-proto` and `cargo check-proto` aliases in
  `.cargo/config.toml`.
  ([#40](https://github.com/asivura/roger/pull/40))
- `team.spawn` RPC method: spawns a teammate as a new Zellij pane via
  `open_command_pane` and tracks it in plugin state. The shim
  receives the new pane id in the response for subsequent
  `team.send` / `team.kill` calls (Phase B #7). Implementation uses a
  deferred-reply pattern (correlation token in `CommandToRun`'s
  context map) because zellij-tile 0.43.1's `open_command_pane` is
  fire-and-forget — the pane id arrives via the `CommandPaneOpened`
  event. From the shim's perspective the RPC remains synchronous.
  New types `SpawnParams` and `SpawnResult` in `roger-proto`,
  plus the `SPAWN_FAILED` error code (-32001).
  ([#44](https://github.com/asivura/roger/pull/44))
- `team.send` and `team.kill` RPC methods: write text into a
  tracked teammate pane (`write_chars_to_pane_id`), and close a
  tracked teammate pane (`close_pane_with_id`). Both are
  synchronous on the RPC (the underlying Zellij calls are
  fire-and-forget). `team.kill` removes the entry from
  `State::teammates` optimistically. New types `SendParams`,
  `KillParams`, and `OkResult` in `roger-proto`. 5 additional unit
  tests (total 22/22 pass). With this PR the four Phase B protocol
  methods (`team.list`, `team.spawn`, `team.send`, `team.kill`) are
  all in; #8 wires `PaneClosed` / `CommandPaneExited` into
  `State::teammates` cleanup as the remaining Phase B item.
  ([#50](https://github.com/asivura/roger/pull/50))
- Pane lifecycle wiring: `Event::CommandPaneExited` marks the
  matching teammate's `exited` flag and records the exit code,
  `Event::PaneClosed(PaneId::Terminal(_))` removes the entry
  (idempotent w.r.t. `team.kill`'s optimistic removal),
  `Event::CommandPaneReRun` clears `exited` / `exit_code` so the
  teammate surfaces as live again. The handler matches on `PaneId`
  variants and only acts on `Terminal(_)` — plugin-pane closures
  are logged and ignored, since we never spawn teammates as
  plugins. New optional wire field `exit_code: Option<i32>` on
  `TeammatePaneInfo`, with `skip_serializing_if = "Option::is_none"`
  so existing `team.list` consumers are unaffected. 4 additional
  unit tests covering the omit/serialize cases (total 26/26 pass).
  With this PR Phase B is functionally complete; the next PRs
  iterate on the existing surface.
  ([#54](https://github.com/asivura/roger/pull/54))

### Changed

- Renamed cargo alias `check-all` → `check-plugin` for consistency
  with `check-shim` and `check-proto`. The old name was misleading
  (it never checked all crates, only the plugin) and was getting
  worse as the workspace grew.
  ([#40](https://github.com/asivura/roger/pull/40))
- `State::teammates` map key changed from `String` (agent id) to
  `u32` (Zellij terminal pane id). Wire format unchanged; the
  pane-id key makes `team.send` / `team.kill` lookups in #7 direct,
  and `team.list`'s `Vec<TeammatePaneInfo>` serialization is
  unaffected. ([#44](https://github.com/asivura/roger/pull/44))
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

### Known limitations

- `team.spawn` has no internal timeout: if Zellij fails to emit
  `CommandPaneOpened` (currently the case when `argv[0]` doesn't
  exist), the corresponding `PendingSpawn` state entry leaks and
  the shim hangs until its own client-side read timeout. Tracked
  as a follow-up watchdog issue.
  ([#44](https://github.com/asivura/roger/pull/44))

### Fixed

- `PermissionType::ReadPaneContents` removed from the plugin's
  initial permission request; it was added in `zellij-tile 0.44`
  but the scaffold targets 0.43.1, so the request was a compile
  error left over from an incomplete prior fix.
  ([#16](https://github.com/asivura/roger/pull/16))

[Unreleased]: https://github.com/asivura/roger/compare/HEAD...main
