# Installing roger

This guide walks through installing roger on a single host: building
both the plugin and the shim, placing each in the right spot, and
configuring Zellij to load the plugin on startup.

End-to-end validation (running Claude Code with `--teammate-mode tmux`
and watching teammates appear as Zellij panes) lives in
[`usage.md`](usage.md). Read this doc first.

## Requirements

- **Rust toolchain** with the `wasm32-wasip1` target. The repo's
  `rust-toolchain.toml` pins both, so a fresh `cargo` invocation will
  install them automatically — you don't need to run `rustup` by
  hand.
- **Zellij ≥ 0.43**. Verified against 0.43.1 (the version `zellij-tile`
  targets today) through 0.44.x. Earlier versions are unsupported.
- **Claude Code** with the agent-teams feature available. The shim
  intercepts Claude Code's `TmuxBackend` shell-outs; without that
  backend in play, the shim has nothing to translate.
- A **single-user dev environment**. The current trust model
  (documented in [`trust-model.md`](trust-model.md)) assumes UID-local
  trust. Don't deploy roger on shared hosts where untrusted code runs
  at the same UID as Claude Code.

## Build

From the repo root:

```bash
cargo build-wasm        # release Wasm plugin → target/wasm32-wasip1/release/roger.wasm
cargo build --release -p roger-shim   # release shim binary → target/release/tmux
```

Both are release builds. The `cargo build-wasm` alias is defined in
`.cargo/config.toml`. The shim binary is named `tmux` deliberately
(it'll shadow the real `tmux` on PATH).

Verify both artifacts exist:

```bash
ls -la target/wasm32-wasip1/release/roger.wasm
ls -la target/release/tmux
```

## Plugin install

Zellij loads plugins from any `file:` URL it can read. The
convention is `~/.config/zellij/plugins/`:

```bash
mkdir -p ~/.config/zellij/plugins
cp target/wasm32-wasip1/release/roger.wasm ~/.config/zellij/plugins/roger.wasm
```

### Option A: persist via `config.kdl` (recommended)

Append to `~/.config/zellij/config.kdl`:

```kdl
load_plugins {
    "file:~/.config/zellij/plugins/roger.wasm"
}
```

The plugin loads on every new Zellij session. Verify with
`zellij list-sessions` and look for the plugin in the running session
(it hides itself on load, so you won't see a visible pane — that's
expected).

### Option B: ad-hoc per session

```bash
zellij plugin -- file:$HOME/.config/zellij/plugins/roger.wasm
```

Useful during development or when you want to test against a freshly-built
plugin without bouncing the session.

## Shim install

The shim binary must sit earlier in `PATH` than the real `tmux`. The
recommended layout is a dedicated directory that you prepend to
`PATH` only when running Claude Code, so the rest of your shell
sessions still use real `tmux` (if you have it installed).

### One-time setup

```bash
mkdir -p ~/.local/roger/bin
cp target/release/tmux ~/.local/roger/bin/tmux
```

### Per-session activation

Either invoke Claude Code with `PATH` modified inline:

```bash
PATH="$HOME/.local/roger/bin:$PATH" claude --teammate-mode tmux
```

Or define a shell function (e.g. in `~/.zshrc` or `~/.bashrc`):

```bash
claude-team() {
    PATH="$HOME/.local/roger/bin:$PATH" claude --teammate-mode tmux "$@"
}
```

Then `claude-team` from any Zellij pane spins up Claude with the
shim active. The shim auto-refuses to run outside a Zellij session
(checks `ZELLIJ_SESSION_NAME`), so accidentally running it elsewhere
is harmless — it exits 2 with a clear error.

### Verify the shim wins on PATH

After activation:

```bash
which tmux
# expected: /home/<you>/.local/roger/bin/tmux
type tmux
# expected: tmux is /home/<you>/.local/roger/bin/tmux
```

If you see `/usr/bin/tmux` or `/opt/homebrew/bin/tmux` instead, the
shim isn't first. Re-check the `PATH=` line.

## Auto-update on rebuild (optional)

If you're iterating on roger, a wrapper script that rebuilds and
re-installs in one shot saves friction:

```bash
#!/usr/bin/env bash
# ~/.local/bin/roger-rebuild
set -euo pipefail
cd "$(git -C ~/Developer/repos/roger rev-parse --show-toplevel)"
cargo build-wasm
cargo build --release -p roger-shim
cp target/wasm32-wasip1/release/roger.wasm ~/.config/zellij/plugins/roger.wasm
cp target/release/tmux ~/.local/roger/bin/tmux
echo "roger: rebuilt and installed."
```

After rebuilding, restart any active Claude Code session that the
shim is talking through (the running shell-out binaries are cached
at exec time, so a new `tmux` won't take effect until the next
`claude --teammate-mode tmux` invocation). The plugin reloads on
every new Zellij session.

## Uninstall

```bash
rm -rf ~/.local/roger/bin
rm -f ~/.config/zellij/plugins/roger.wasm
```

Remove the `load_plugins { ... }` block from `~/.config/zellij/config.kdl`
if you used Option A.

## Troubleshooting

### `roger-shim: not running inside Zellij`

You ran `tmux <subcommand>` from a shell that isn't a Zellij pane.
The shim refuses (exit code 2). Either:
- Open a Zellij session and run from a pane inside it, or
- Revert your `PATH` so real `tmux` wins (the shim is only meant
  to be active when Claude Code is invoking it).

### `which tmux` shows the wrong binary

`PATH` doesn't have `~/.local/roger/bin` first. Re-check the
activation method you used (inline `PATH=`, shell function, etc.)
and confirm it's in effect for the shell that launched Claude Code.

### Plugin loaded but no panes spawn

Check Zellij's plugin log:

```bash
tail -f $(zellij setup --check 2>&1 | grep '"plugin_log_dir"' | sed 's/.*: //; s/"//g')/roger.log
```

…or wherever Zellij is writing plugin logs on your install (the
location varies by Zellij version). The shim emits `eprintln!`
lines on the host side (visible in the shell that invoked it); the
plugin emits to the Zellij log.

If neither side shows activity when you run `tmux display-message
-p '#{pane_id}'`, the shim isn't actually being invoked — go back to
`which tmux`.

### `zellij pipe` hangs

`zellij pipe` blocks until the plugin replies. If the plugin isn't
loaded (or crashed on load), the call hangs until Zellij eventually
times out. Symptom: shim subcommands hang too. Confirm the plugin
loaded by checking the plugin log; reload via `zellij plugin --
file:...`.

### Newer Zellij version breaks the plugin

`zellij-tile` is pinned to 0.43.1 in `plugin/Cargo.toml`. Newer
Zellij versions may not be fully compatible until the dep is
bumped — that's tracked as #13. If you're on Zellij ≥ 0.45 and
the plugin won't load, file an issue with the Zellij version
included.

## Where to next

See [`usage.md`](usage.md) for the workflow once everything's
installed.
