# Installing roger

This guide walks through installing roger on a single host: building
both the plugin and the shim, placing each in the right spot, and
configuring Zellij to load the plugin on startup.

End-to-end validation (running Claude Code with `--teammate-mode tmux`
and watching teammates appear as Zellij panes) lives in
[`usage.md`](usage.md). Read this doc first.

## Requirements

- **`rustup`**. The repo's `rust-toolchain.toml` pins stable Rust and
  the `wasm32-wasip1` target — both are auto-installed on the first
  `cargo` invocation, but **only when `cargo` is the rustup shim**.
  A bare `cargo` from your distro's package manager will ignore
  `rust-toolchain.toml` and produce a build failure. Check:

  ```bash
  rustup --version    # any 1.x is fine; not present → install rustup
  which cargo         # should resolve under ~/.cargo/bin (rustup shim)
  ```

  If `rustup` isn't installed:

  ```bash
  curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | \
      sh -s -- -y --default-toolchain none --no-modify-path
  . "$HOME/.cargo/env"
  ```

- **`zellij`**, version 0.44+. Earlier versions aren't supported (the
  plugin ABI changed at 0.44). Check:

  ```bash
  zellij --version    # → zellij 0.44.x
  ```

- **Claude Code** with the agent-teams feature, configured to use
  `TmuxBackend`. The shim intercepts Claude Code's `tmux` shell-outs
  — without `TmuxBackend` in play, the shim has nothing to translate.
  See [`usage.md`'s troubleshooting](usage.md#troubleshooting) for
  how to confirm Claude is actually routing through tmux on your
  platform.

- A **single-user dev environment**. The trust model assumes UID-local
  trust; see [`trust-model.md`](trust-model.md). Don't deploy roger on
  shared hosts where untrusted code runs at the same UID as Claude
  Code.

## Build

From the repo root:

```bash
cargo build-wasm        # release Wasm plugin → target/wasm32-wasip1/release/roger.wasm
cargo build-shim        # release shim binary → target/release/tmux
```

Both are release builds; `build-wasm` and `build-shim` are aliases
defined in `.cargo/config.toml`. The shim binary is named `tmux`
deliberately (it'll shadow the real `tmux` on PATH).

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

If `~/.config/zellij/config.kdl` doesn't exist yet (fresh Zellij
install), create it with `zellij setup --dump-config >
~/.config/zellij/config.kdl` first.

Append to `~/.config/zellij/config.kdl`:

```kdl
load_plugins {
    "file:~/.config/zellij/plugins/roger.wasm"
}
```

The plugin loads on every new Zellij session. Verify with
`zellij list-sessions` and look for the plugin in the running session
(it hides itself on load, so you won't see a visible pane — that's
expected). The smoke test in [Verification](#verification) below
confirms the plugin is actually loaded.

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

## Verification

After both the plugin and shim are installed, run two checks:

### 1. Shim wins on PATH

After activating per the chosen pattern above:

```bash
which tmux
# expected: /home/<you>/.local/roger/bin/tmux
type tmux
# expected: tmux is /home/<you>/.local/roger/bin/tmux
```

If you see `/usr/bin/tmux` or `/opt/homebrew/bin/tmux` instead, the
shim isn't first. Re-check the `PATH=` line.

### 2. Plugin loaded and reachable

Start (or attach to) a Zellij session, then from a pane inside it:

```bash
echo '{"method":"team.list","id":"smoke","params":{}}' | \
    zellij pipe --name roger-rpc \
        --plugin "file:$HOME/.config/zellij/plugins/roger.wasm"
```

Expected output (no teammates yet):

```json
{"id":"smoke","result":{"panes":[]}}
```

If you get the JSON reply, both the plugin and the pipe transport
are working — you're done. Common alternative outcomes:

- **`failed to load plugin from instance / could not find exported
  function`** — your plugin was built against the wrong zellij-tile
  version for your Zellij. Confirm `zellij --version` matches the
  Zellij that built your `roger.wasm`. If you bumped Zellij after
  building, `cargo build-wasm` again.
- **Interactive permission prompt** — Zellij is asking you to grant
  the plugin its requested permissions. Approve via the prompt
  (this is a one-time step per plugin URL); subsequent loads use
  the cached grant at `~/.cache/zellij/permissions.kdl`.
- **Timeout / `Action CliPipe did not complete within 1s`** — common
  on the very first plugin load after the permission grant. Retry
  the command; once the plugin instance is warm the reply is
  near-instant.

## Auto-update on rebuild (optional)

If you're iterating on roger, a wrapper script that rebuilds and
re-installs in one shot saves friction:

```bash
#!/usr/bin/env bash
# ~/.local/bin/roger-rebuild
set -euo pipefail
ROGER_DIR="${ROGER_DIR:-$HOME/Developer/repos/roger}"
cd "$ROGER_DIR"
cargo build-wasm
cargo build-shim
cp target/wasm32-wasip1/release/roger.wasm ~/.config/zellij/plugins/roger.wasm
cp target/release/tmux ~/.local/roger/bin/tmux
echo "roger: rebuilt and installed from $ROGER_DIR."
```

`ROGER_DIR` is overridable so anyone who clones the repo elsewhere
can `ROGER_DIR=/some/path roger-rebuild` without editing the script.

After rebuilding, restart any active Claude Code session that the
shim is talking through (the running shell-out binaries are cached
at exec time, so a new `tmux` won't take effect until the next
`claude --teammate-mode tmux` invocation). The plugin reloads on
every new Zellij session.

## Uninstall

```bash
# artifacts
rm -rf ~/.local/roger
rm -f ~/.config/zellij/plugins/roger.wasm
# cached permission grants and plugin wasm cache
rm -f ~/.cache/zellij/permissions.kdl
rm -rf ~/.cache/zellij/*/file:* ~/.cache/zellij/file:*
```

Also remove:

- The `load_plugins { ... }` block from `~/.config/zellij/config.kdl`
  if you used Option A.
- Your shell function (`claude-team`) or PATH-modifying `.zshrc` /
  `.bashrc` line.
- The optional `roger-rebuild` script at `~/.local/bin/roger-rebuild`
  if you installed it.

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

Plugin `eprintln!` lines land in Zellij's runtime log:

```bash
tail -f /tmp/zellij-$(id -u)/zellij-log/zellij.log
```

(Linux path; macOS / BSD users may need to check `zellij setup
--check` for their cache dir.) The shim emits `eprintln!` lines on
the host side instead — those are visible in the shell that
invoked it.

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
