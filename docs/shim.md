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

### Shell-prompt flash on teammate spawn

`tmux new-session` / `new-window` / `split-window` create an *empty*
pane in real tmux; TmuxBackend then issues a separate `send-keys` to
launch the teammate process. But the plugin's `team.spawn` requires
non-empty `argv` (an empty argv has no command to actually run, so
the plugin rejects it with `SPAWN_FAILED`).

The shim works around this by spawning the user's `$SHELL` (with a
`/bin/bash` fallback) as a placeholder. The follow-up `send-keys`
then types `claude --agent-id …` into the shell, which executes the
teammate. End-to-end this matches real tmux's behavior — pane id is
returned immediately, teammate process starts in the next tick — at
the cost of a brief shell-prompt flash that Claude Code doesn't read
(it only consumes pane ids, not pane stdout).

Two alternative designs were considered and deferred:
- Shim-side deferred spawn (buffer the new-session params until
  send-keys delivers the real command). Cleaner runtime but adds
  cross-invocation state to a one-shot CLI, which is a tarpit.
- `bash -c "read line; exec $line"` bootstrap. Clever but fragile
  under shell metacharacters in argv.

If the shell-prompt flash becomes a real UX issue, the deferred
design is the right next step.

### The `command` field on `team.list` shows `$SHELL`

Because the shim spawns `$SHELL` as the placeholder, the plugin's
`TeammatePaneInfo.command` ends up as `/bin/zsh` or `/bin/bash`
rather than the eventual `claude --agent-id ...`. The pane *title*
(set from `team.spawn`'s `name` parameter) is the right human-readable
signal. Accepting the wire-format lie for v0.1; a future PR could
update `command` post-send-keys.

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
