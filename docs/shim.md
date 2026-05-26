# roger-shim — the `tmux`-compatible CLI

`roger-shim` is the host binary that ships as `target/release/tmux`.
When it sits earlier in `PATH` than the real `tmux`, every `tmux
<subcommand>` invocation from Claude Code's `TmuxBackend` lands here.
The shim translates the tmux invocation into a JSON-RPC call against
the [roger plugin](rpc-protocol.md) via `zellij pipe`, and prints
back whatever shape `TmuxBackend` expects from real tmux.

Phase B shipped the plugin side; this document covers the shim side
(closes #9 + #10).

## Prerequisites

- The shim must be invoked **inside a Zellij session**. It checks
  `ZELLIJ_SESSION_NAME` at startup and exits with code 2 if that
  env var is unset. Run anything that needs the shim from a Zellij
  pane (which is also where Claude Code itself runs).
- The roger plugin must be loaded into the active Zellij session.
  See the project README's "Loading the plugin into Zellij" section.
- `zellij` must be on PATH. The shim invokes `zellij pipe --name
  roger-rpc` for every RPC.

## Command surface

### Real translations (#9)

These map to plugin RPC calls and return tmux-shaped output.

| tmux invocation | RPC | Output |
|---|---|---|
| `tmux display-message -p '#{pane_id}'` | `team.list` (only on fallback path) | `%<n>` (from `ZELLIJ_PANE_ID` env, or first tracked pane, or `%0`) |
| `tmux has-session -t <name>` | none | exit 0 (we manage one Zellij session) |
| `tmux new-session -d -s <name> -n <window> -P -F '#{pane_id}'` | `team.spawn` | `%<n>` |
| `tmux new-window -t <session> -n <name> -P -F '#{pane_id}'` | `team.spawn` | `%<n>` |
| `tmux split-window -t <pane> [-h|-v] [-l <pct>] -P -F '#{pane_id}'` | `team.spawn` | `%<n>` |
| `tmux list-panes -t <window> -F '#{pane_id}'` | `team.list` | newline-separated `%<n>` per pane |
| `tmux send-keys -t <pane> <text>... [Enter]` | `team.send` | (no stdout; exit 0 on success) |
| `tmux kill-pane -t <pane>` | `team.kill` | (no stdout; exit 0 on success, also on "already gone") |

### Cosmetic stubs (#10)

These are accepted silently — Zellij does its own layout, so the
TmuxBackend's chrome calls have no meaningful analogue:

- `tmux select-pane -t <pane> -P 'bg=...,fg=...'`
- `tmux select-pane -t <pane> -T '<title>'`
- `tmux set-option …`, `tmux set-window-option …`, `tmux set-environment …`
- `tmux select-layout -t <window> {main-vertical,tiled}`
- `tmux resize-pane -t <pane> -x <n>%`
- `tmux break-pane`, `tmux join-pane`

### Catch-all

Any unrecognized subcommand prints a one-line warning to stderr and
exits 0. This is deliberate — future Claude Code additions will
degrade rather than crash. The warning shows in plugin logs so an
operator inspecting them sees what was attempted.

## Global flags

Real tmux accepts `-S <socket-path>` and `-L <socket-name>` as global
flags before the subcommand. Claude's TmuxBackend occasionally emits
them. The shim accepts them and ignores their values (we don't have
sockets; we have one Zellij session per shim invocation).

## v0.1 limitations

### Spawn-then-exec placeholder (no longer flashes a shell prompt)

`tmux new-session` / `new-window` / `split-window` create an *empty*
pane in real tmux; TmuxBackend then issues a separate `send-keys` to
launch the teammate process. But the plugin's `team.spawn` requires
non-empty `argv` (an empty argv has no command to actually run, so
the plugin rejects it with `SPAWN_FAILED`).

The shim works around this by spawning a small shell snippet:

```
$SHELL -c 'IFS= read -r cmd && eval "exec $cmd"'
```

The shell waits for a single line on stdin (which the follow-up
`send-keys` delivers — typically `claude --agent-id …`), then
`eval`s `exec $cmd`, replacing the shell with the actual teammate
process. Because of the `exec`, **the pane's main process is the
teammate's claude, not a long-lived shell.** When claude exits, the
pane process exits, Zellij emits `CommandPaneExited`, and the
plugin's lifecycle handler decides whether to auto-close the pane
(see "Auto-close on clean exit" below).

This trades the "brief shell-prompt flash" of the older
`argv = [$SHELL]` form for a slightly cleverer shell snippet —
which is fine because the trust contract on `send-keys` text limits
the producers to roger-shim itself (see
[`trust-model.md`](trust-model.md)).

### Auto-close on clean exit (kept open on error)

When a teammate's claude process exits, the plugin reacts based on
the exit code:

- **Exit code 0** (clean): plugin removes the entry from
  `State::teammates` and calls `close_pane_with_id` — the pane goes
  away. No clutter accumulates after `/team` shutdowns or normal
  teammate completions.
- **Exit code non-zero** (error): plugin marks the entry
  `exited: true` and records the code, but keeps the pane open. The
  operator can read scrollback to debug the crash, then close the
  pane manually (whatever your Zellij keybinding config uses for
  close-pane — stock default is the `Ctrl+p` then `x` chord) — that
  fires `PaneClosed` and
  cleans up state.
- **Exit code not reported by Zellij** (None): conservative —
  treated like a non-zero exit (pane stays open). Rare on Linux but
  possible if the process was killed by a signal the OS didn't
  surface.

This means after a `/team` shutdown where every teammate finishes
cleanly, the layout returns to just the lead pane with no leftover
panes to close. Pre-#63 behavior — `argv = [$SHELL]` left the shell
running with a prompt after claude exited — is no longer relevant.

### The `command` field on `team.list` shows the spawn snippet

Because the shim's `team.spawn` argv is the `bash -c '…'`
spawn-then-exec snippet, the plugin's `TeammatePaneInfo.command`
ends up as something like `/bin/bash -c IFS= read -r cmd …` rather
than the eventual `claude --agent-id …`. The pane *title* (set
from `team.spawn`'s `name` parameter) is the right human-readable
signal. Accepting the wire-format lie for v0.1; a future PR could
update `command` after `send-keys` delivers the real text.

### No real integration test in CI

Phase C builds the shim's structure (clap CLI, RPC payload
construction, output formatting) and unit-tests each piece in
isolation. Real end-to-end exercise — `claude --teammate-mode tmux`
against a Zellij session with the plugin loaded — is **Phase D
(#11)**. Don't expect the shim's behavior to be fully validated
until that lands.

## Debug recipes

### Verify the shim sees Zellij

```bash
ZELLIJ_SESSION_NAME=verify-debug ZELLIJ_PANE_ID=99 ./target/release/tmux display-message -p '#{pane_id}'
```

Should print `%99` and exit 0. If you see the "not running inside
Zellij" error, the env var isn't being passed through.

### Verify RPC plumbing without the plugin

The shim shells out to `zellij pipe`. With no plugin loaded the call
will hang on the pipe-write side until Zellij times it out. That's
not a shim bug — it's the absence of a listener.

### Inspect what TmuxBackend would invoke

`strace -f -e execve -s 4096 claude --teammate-mode tmux ...` shows
every shell-out TmuxBackend produces. Useful when adding handling for
a new subcommand or debugging a new chrome surface.
