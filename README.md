# roger

A Zellij plugin for orchestrating Claude Code agent teams as native panes.

Named after **Roger Penrose**, whose 1974 mathematics of aperiodic
tilings was independently achieved by medieval Islamic mosaicists 500
years earlier ([Lu & Steinhardt, *Science* 2007][lu-steinhardt]). That
same zellij tilework tradition is what [Zellij][zellij] the multiplexer
takes its name from. Local rules, locally placed, producing globally
coherent patterns: the mathematics of zellij tilings, of Penrose's
kites and darts, and of multi-agent coordination, are the same
mathematics.

## Status

Early. The repository scaffolding is in place; the plugin code is
being built out incrementally. The first milestone is a Wasm plugin
that compiles and loads in Zellij 0.44+, exposing a stub JSON-RPC
interface that the rest of the architecture can be built against.

## What it does (and why)

Claude Code's [agent teams][claude-agent-teams] feature spawns multiple
cooperating Claude agents into separate terminal panes so the operator
can watch them work in parallel. On macOS the backend uses iTerm2; on
Linux it uses tmux; in headless environments it falls back to an
in-process backend that gives zero visibility (teammates are
sidechains of the lead session, not separate processes).

`roger` adds Zellij as a first-class option. Teammates run as real
Zellij panes, observable through the Zellij web client from any
WARP-enrolled or otherwise authenticated browser. Per-session URLs,
multiplayer cursors, read-only share tokens for the iPad-as-monitor
case: capabilities tmux structurally lacks.

This complements (does not replace) tmux. If you already have a
working tmux + Claude Code teammate workflow, you don't need `roger`.
If you want the same thing accessible from any browser, with all the
properties Zellij's web client gives you, this is what makes that
possible.

## Architecture

Two crates, workspace layout (only the first is currently scaffolded):

| Crate | What it is |
|---|---|
| `plugin/` | A Zellij Wasm plugin. Hides on load, subscribes to pane lifecycle events, exposes a JSON-RPC interface over `zellij pipe` for spawning + addressing teammate panes. |
| `shim/` | A CLI binary that sits earlier in `PATH` than the real `tmux` and translates Claude Code's `TmuxBackend` shell-outs into RPC calls to the plugin. Claude doesn't have to know Zellij exists. |

The shim approach is borrowed from
[stanislc/zellij-claude-teams][stanislc], which proved out that the
real tmux command surface Claude Code uses is small (~5 commands do
real work). `roger`'s plugin layer cleans up the parts of that
approach that are ugly when you can call into Zellij directly (no
FIFO-per-pane hack for `write-chars`, no `move-focus` workaround for
focus stealing, native pane IDs without translation).

## Building

The repo ships a `rust-toolchain.toml` that pins stable Rust and
`wasm32-wasip1`, so `cargo` auto-installs both on first invocation.
The workspace has two crates, each with its own canonical command
(aliased in `.cargo/config.toml`):

```bash
cargo build-wasm    # release Wasm plugin -> target/wasm32-wasip1/release/roger.wasm
cargo build-shim    # release shim binary  -> target/release/tmux
cargo check-all     # plugin clippy
cargo check-shim    # shim clippy
```

Loading the plugin into Zellij:

```bash
zellij plugin -- file:$PWD/target/wasm32-wasip1/release/roger.wasm
# Or persist via `load_plugins { "file:..." }` in ~/.config/zellij/config.kdl
```

Detailed install instructions for the shim (PATH ordering, env
setup) land in [#12](https://github.com/asivura/roger/issues/12).
See [CONTRIBUTING.md](CONTRIBUTING.md) for the verify-before-pushing
recipe.

## Why "roger"?

In 2007, Peter Lu and Paul Steinhardt published a paper in *Science*
showing that the geometric tilework on the 15th-century Darb-i Imam
shrine in Isfahan was a near-perfect quasi-crystalline Penrose tiling
— constructed five centuries before Penrose proved the same
mathematics in the West. The Maâlem craftsmen who laid those tiles
didn't know they were doing quasi-crystallography; they just knew the
rules for placing each tile in relation to its neighbors. Local
constraints, no central plan, global coherence.

That is also Zellij the multiplexer (panes composing into a workspace)
and that is also Claude Code agent teams (specialized agents
coordinating into a result). Naming the plugin `roger` honors the
mathematician who later formalized what zellij artisans had already
discovered.

## Inspirations

- [`mrshu/zjctl`][zjctl] — the canonical Zellij Wasm plugin + CLI architecture
- [`ishefi/zellaude`][zellaude] — Claude Code hook integration pattern
- [`stanislc/zellij-claude-teams`][stanislc] — the tmux-binary shim pattern, validated in production
- [`KiryuuLight/zellij-attention`][zellij-attention] — Notification/Stop hook UX in Zellij tab badges
- [anthropics/claude-code#24122][issue-24122] and [#31901][issue-31901] — upstream feature requests this work side-steps
- [anthropics/claude-code#26572][issue-26572] — the `CustomPaneBackend` proposal that would make this work obsolete if it ships

## License

[MIT](LICENSE).

[lu-steinhardt]: https://paulsteinhardt.org/wp-content/uploads/2023/01/LuSteinhardt2007.pdf
[zellij]: https://zellij.dev/
[claude-agent-teams]: https://docs.claude.com/en/docs/claude-code/agent-teams
[zjctl]: https://github.com/mrshu/zjctl
[zellaude]: https://github.com/ishefi/zellaude
[stanislc]: https://github.com/stanislc/zellij-claude-teams
[zellij-attention]: https://github.com/KiryuuLight/zellij-attention
[issue-24122]: https://github.com/anthropics/claude-code/issues/24122
[issue-31901]: https://github.com/anthropics/claude-code/issues/31901
[issue-26572]: https://github.com/anthropics/claude-code/issues/26572
