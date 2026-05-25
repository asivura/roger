# Roadmap

## Vision

`roger` makes Claude Code's [agent teams][teams] visible in [Zellij][zellij]
the way they're visible in tmux on the Mac, as real, browser-accessible
panes you can attach to from anywhere. We complement tmux, we don't
replace it.

When Claude's `TmuxBackend` shells out to `tmux split-window` to spawn a
teammate, the `roger` shim CLI intercepts the call and routes it to the
`roger` Wasm plugin via `zellij pipe`. The plugin calls
`open_command_pane` on the Zellij side and returns the pane id. To
Claude nothing has changed; to the operator, teammates now live in
Zellij panes observable through the [Zellij web client][zellij-web]
from any authenticated device.

## Architecture

Two crates:

- **`plugin/`** — the Zellij Wasm plugin. Hides on load, subscribes to
  pane lifecycle events, exposes a JSON-RPC interface over `zellij
  pipe`.
- **`shim/`** — a CLI binary named `tmux` that lives earlier in `PATH`
  than the real `tmux`. Translates Claude's tmux invocations into RPC
  calls to the plugin.

The shim approach is borrowed from [stanislc/zellij-claude-teams][stanislc],
which proved out that the real tmux command surface Claude Code uses is
small (~6 commands do real work). `roger`'s plugin layer cleans up the
parts of that approach that are ugly when you can call into Zellij
directly: no FIFO-per-pane hack for `write-chars`, no `move-focus`
workaround for focus stealing, native pane ids without translation.

See [README.md](README.md) for the longer story and the
"why-roger-the-name" story.

## Phases

Each phase is a [GitHub milestone][milestones]. Each milestone is a
small number of issues. We merge one PR per issue, on feature
branches, rebased onto `main` (no merge commits, no squash).

### [Phase A: Foundation][m1]

Goals: governance and structure in place before any feature work.

- [#1][i1] ROADMAP.md (this document)
- [#2][i2] CI: build + Rust lint + commit-message lint
- [#3][i3] PR template, `CONTRIBUTING.md`, `CHANGELOG.md`, `RELEASING.md`
- [#4][i4] Cargo workspace restructure

### [Phase B: Plugin core][m2]

Goals: the plugin can spawn, send to, kill, and list panes via RPC.

- [#5][i5] RPC protocol design + `team.list`
- [#6][i6] `team.spawn`
- [#7][i7] `team.send` + `team.kill`
- [#8][i8] Pane lifecycle tracking

### [Phase C: Shim CLI][m3]

Goals: a `tmux`-named binary that translates the six tmux commands
Claude's `TmuxBackend` actually uses into RPC calls to the plugin.

- [#9][i9] Translate the real commands
- [#10][i10] Stub the cosmetic commands and detect when not running
  inside Zellij

### [Phase D: Integration][m4]

Goals: end-to-end verification.

- [#11][i11] Smoke test: `claude --teammate-mode tmux` spawns a real
  teammate Zellij pane
- [#12][i12] Install + usage docs

### [Phase E: Observability][m5]

Goals: status indicators per teammate via Claude Code hooks.

- [#13][i13] Bump `zellij-tile` to 0.44 for `ReadPaneContents`, add
  Claude Code hook integration, render state as tab title decoration

### [Phase F: Release][m6]

Goals: a v0.1.0 tag with a downloadable Wasm artifact.

- [#14][i14] v0.1.0 release: tag, CHANGELOG, GitHub Release, artifacts

## Versioning

[Semantic Versioning](https://semver.org/spec/v2.0.0.html) for the
crate version in `Cargo.toml`.

- **v0.1.0** — first usable release. Spawning works end-to-end via the
  shim + plugin path. Observability may or may not be in.
- **v0.2.0** — observability layer (Claude Code hooks, status badges).
- **v1.0.0** — either when the upstream [`CustomPaneBackend`][issue-26572]
  proposal lands and we pivot to it, or when the shim approach has
  been proven stable across several Claude Code minor versions.

Every tagged release ships with:

- A `CHANGELOG.md` entry following [Keep a Changelog][keepacl] format
- A GitHub Release with notes and binary artifacts attached
- A `Cargo.toml` `version` bump matching the tag

See [RELEASING.md](RELEASING.md) (coming with [#3][i3]) for the
release process.

## Contributing

See [CONTRIBUTING.md](CONTRIBUTING.md) (coming with [#3][i3]).

Highlights of the workflow:

- Branch naming: `feat/X`, `fix/X`, `docs/X`, `chore/X`, `refactor/X`
- Commit messages: [Conventional Commits][cc]. CI enforces this on
  every PR commit ([#2][i2]).
- Merge style: **rebase only**, no squash, no merge commits. The repo
  is configured to allow only "Rebase and merge" in the UI.
- Branch protection: `main` requires PRs (even from the maintainer),
  linear history, and conversation resolution before merge.

[teams]: https://docs.claude.com/en/docs/claude-code/agent-teams
[zellij]: https://zellij.dev/
[zellij-web]: https://zellij.dev/documentation/web-client.html
[stanislc]: https://github.com/stanislc/zellij-claude-teams
[issue-26572]: https://github.com/anthropics/claude-code/issues/26572
[keepacl]: https://keepachangelog.com/en/1.1.0/
[cc]: https://www.conventionalcommits.org/
[milestones]: https://github.com/asivura/roger/milestones
[m1]: https://github.com/asivura/roger/milestone/1
[m2]: https://github.com/asivura/roger/milestone/2
[m3]: https://github.com/asivura/roger/milestone/3
[m4]: https://github.com/asivura/roger/milestone/4
[m5]: https://github.com/asivura/roger/milestone/5
[m6]: https://github.com/asivura/roger/milestone/6
[i1]: https://github.com/asivura/roger/issues/1
[i2]: https://github.com/asivura/roger/issues/2
[i3]: https://github.com/asivura/roger/issues/3
[i4]: https://github.com/asivura/roger/issues/4
[i5]: https://github.com/asivura/roger/issues/5
[i6]: https://github.com/asivura/roger/issues/6
[i7]: https://github.com/asivura/roger/issues/7
[i8]: https://github.com/asivura/roger/issues/8
[i9]: https://github.com/asivura/roger/issues/9
[i10]: https://github.com/asivura/roger/issues/10
[i11]: https://github.com/asivura/roger/issues/11
[i12]: https://github.com/asivura/roger/issues/12
[i13]: https://github.com/asivura/roger/issues/13
[i14]: https://github.com/asivura/roger/issues/14
