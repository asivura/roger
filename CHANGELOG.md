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
- `docs/trust-model.md`: normative v0.1 trust-model documentation
  (UID-local, single-user assumption; `team.send`'s extra surface
  on the PTY trust contract; what's a trust boundary vs not).
  Linked from `docs/rpc-protocol.md`. Closes #53.
  ([#56](https://github.com/asivura/roger/pull/56))
- Spawn watchdog: `pending_spawns` entries now expire after
  `SPAWN_WATCHDOG_TTL_SECS` (10s) if Zellij never emits the
  matching `CommandPaneOpened`. The plugin uses `set_timeout` +
  `Event::Timer` (the wasm-safe time primitives from
  `zellij-tile 0.43.1`) to tick every 5s and sweep aged entries,
  replying `SPAWN_FAILED` ("timed out waiting for
  CommandPaneOpened") for each. Watchdog rearms only while
  `pending_spawns` is non-empty, so an idle plugin doesn't wake
  up unnecessarily. Closes #45 — removes the only entry from
  the previous `Known limitations` section.
  ([#57](https://github.com/asivura/roger/pull/57))
- `docs/install.md` — full operator install guide. Covers
  prerequisites, building the plugin + shim, placing artifacts,
  Zellij `config.kdl` autoload, PATH-shadow setup for the shim,
  per-session activation patterns (inline `PATH=` or shell
  function), verification (`which tmux`), an optional rebuild
  script, uninstall, and a troubleshooting section keyed to the
  most common setup failure modes.
- `docs/usage.md` — operator workflow doc. Covers the basic
  spawn-watch-cleanup flow, lifecycle semantics, how to inspect
  the plugin's state with `zellij pipe`, resuming teammate
  sessions from non-Zellij shells, re-running exited commands,
  reading plugin logs, and a separate troubleshooting section for
  runtime issues (garbled keys, stuck spawns hitting the
  watchdog, unrecognized subcommands).
- `README.md` now links the install + usage + protocol + trust
  docs from the Building section so newcomers have a path.
  Closes #12.
  ([#59](https://github.com/asivura/roger/pull/59))
- `roger-shim` is now a real binary instead of a stub. Implements
  the eight real tmux translations Claude Code's `TmuxBackend`
  uses — `display-message`, `has-session`, `new-session`,
  `new-window`, `split-window`, `list-panes`, `send-keys`,
  `kill-pane` — each routed through the matching plugin RPC
  method (`team.list`, `team.spawn`, `team.send`, `team.kill`).
  Cosmetic ops (`select-pane`, `set-option`, layout commands,
  etc.) are accepted silently; unknown subcommands log a warning
  to stderr and exit 0 so future Claude additions degrade rather
  than crash. Outside a Zellij session (`ZELLIJ_SESSION_NAME`
  unset) the shim exits 2 with a clear error. `-S <socket>` and
  `-L <name>` global tmux flags are accepted and ignored. New
  modules `shim/src/rpc.rs` (JSON-RPC client over `zellij pipe`),
  `shim/src/pane_id.rs` (parse / render `%<n>`), and
  `shim/src/commands/*.rs` (per-subcommand handlers). 27 unit
  tests covering argv parsing, key-token rendering, pane-id
  parsing, and global-flag stripping. New `docs/shim.md`
  documenting the command surface and v0.1 limitations (notably
  the spawn-shell-then-send-keys workaround for the empty-pane
  semantic mismatch). End-to-end exercise against Claude
  `--teammate-mode tmux` is Phase D (#11). Closes #9, closes #10.
  ([#58](https://github.com/asivura/roger/pull/58))

### Fixed

- **Plugin now actually loads in Zellij 0.44+.** Discovered
  empirically while installing on the lab box (Zellij 0.44.3,
  aarch64) — the plugin produced
  `failed to load plugin from instance / could not find exported
  function`. Two compounding causes:
  - The transitive `zellij-utils` protobuf wire types changed
    between 0.43 and 0.44; a 0.43-built plugin can't decode events
    from a 0.44 host. The `zellij-tile` public API (the
    `register_plugin!` macro, `ZellijPlugin` trait, shim
    functions) is byte-identical between versions — only the
    protobuf payloads moved. Bumped the dep to `"0.44"`. No
    source change required.
  - Zellij's plugin host calls `_start` to initialize the wasm
    instance. The wasi-sdk crt only wraps `fn main()` into a
    `_start` export for binary crates on `wasm32-wasip1`; the
    previous `[lib] crate-type = ["cdylib"]` produced no `_start`.
    Switched the plugin to a binary target: renamed
    `plugin/src/lib.rs` → `plugin/src/main.rs`, replaced `[lib]`
    with `[[bin]] name = "roger" path = "src/main.rs"`, and
    updated the `--lib` → `--bin roger` flag in
    `.cargo/config.toml` and `.github/workflows/ci.yml`.

  Verified by `wasm-tools print` comparing pre/post exports against
  a known-working built-in Zellij plugin — post-fix matches
  exactly (`_start`, `__main_void`, `load`, `update`, `pipe`,
  `render`, `plugin_version`, `memory`). Side-benefit: the panic
  hook installed by `register_plugin!`'s `fn main()` actually runs
  now (it was dead code under the `[lib]` shape).
  ([#60](https://github.com/asivura/roger/pull/60))

### Changed

- Extracted `parse_params<T>` helper for the params-deserialization
  prelude shared by `team.spawn` / `team.send` / `team.kill`. Three
  identical match arms collapse to a single `Self::parse_params`
  call per handler. No behavior change; error-message format is
  preserved. Closes #51.
  ([#56](https://github.com/asivura/roger/pull/56))
- Dropped the dead `INTERNAL_ERROR` fallback branches from the
  three result-serialization sites whose result types
  (`OkResult { ok: bool }`, `SpawnResult { pane_id: u32 }`,
  `TeamListResult { panes: Vec<TeammatePaneInfo> }`) cannot fail
  to serialize. Each site now uses `.expect("… serializes
  infallibly")` to assert the invariant loudly. The `reply()`
  fallback for the top-level `Response` envelope is unchanged.
  Closes #52.
  ([#56](https://github.com/asivura/roger/pull/56))
- Dropped `EventType::PaneUpdate` from the plugin's subscription
  list. The plugin never matched it in `update()` (it fell through
  the `_ => {}` arm) and we have no use case for pane title /
  dimension change tracking in Phase B. Avoiding speculative
  subscriptions per the project's "don't design for hypothetical
  future requirements" rule. Closes #55.
  ([#56](https://github.com/asivura/roger/pull/56))
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

(none currently — the `team.spawn` internal-timeout limitation from
PR #44 was resolved by PR #57.)

### Fixed

- `PermissionType::ReadPaneContents` removed from the plugin's
  initial permission request; it was added in `zellij-tile 0.44`
  but the scaffold targets 0.43.1, so the request was a compile
  error left over from an incomplete prior fix.
  ([#16](https://github.com/asivura/roger/pull/16))

[Unreleased]: https://github.com/asivura/roger/compare/HEAD...main
