# Using roger

This guide assumes you've already followed [`install.md`](install.md):
plugin loaded into your Zellij session, shim binary at
`~/.local/roger/bin/tmux`, and you have an activation method
(inline `PATH=` or a shell function) for invoking Claude Code with
the shim earlier on `PATH`.

For the architecture behind what's happening, see the project
[`README`](../README.md). For wire-level detail of every plugin RPC,
see [`rpc-protocol.md`](rpc-protocol.md). For the trust model — what
you can and can't assume about callers — see
[`trust-model.md`](trust-model.md).

## The basic workflow

From a Zellij pane:

```bash
claude-team
```

…where `claude-team` is the shell function you defined in
`install.md` (or the inline `PATH=` form if you prefer). Claude Code
starts as normal, but with `--teammate-mode tmux` active and the
shim sitting between Claude and Zellij.

When you ask Claude to spawn a teammate (via the Agent tool with
`team_name`, or via a `/team` slash command, depending on your
Claude version), Claude shells out to `tmux new-session ...` (which
is really the roger shim). The shim translates this into a
`team.spawn` RPC against the plugin, the plugin opens a Zellij pane
with the teammate's `claude` process inside, and the new pane
appears in your tab.

You can watch teammates work in real time, switch focus between
them (`Ctrl-p` then arrow keys, or whatever your Zellij keybinding
config uses), and let Claude orchestrate them through `tmux
send-keys` / `tmux kill-pane` as needed.

### Lifecycle in one paragraph

A teammate pane lives from `team.spawn` until either Claude asks
the shim to kill it (`tmux kill-pane` → `team.kill`) or you close
the pane manually in Zellij (which emits `PaneClosed` and removes
the entry from the plugin's tracking map). If the teammate
process exits on its own (e.g. it ran a one-shot prompt and
finished), Zellij emits `CommandPaneExited` — the plugin marks
the entry `exited: true` and records the exit code, but keeps the
pane open so you can read the scrollback. To clean up, close the
pane in Zellij.

## What the shim translates

Every `tmux <subcommand>` invocation from Claude Code goes through
the shim. The mapping is documented in [`shim.md`](shim.md). The
short version: `display-message`, `has-session`, `new-session`,
`new-window`, `split-window`, `list-panes`, `send-keys`,
`kill-pane` map to RPC calls; cosmetic commands
(`select-pane`, `set-option`, `select-layout`, etc.) accept
silently. Any subcommand we don't recognize logs a one-line warning
to stderr and exits 0 — future Claude Code additions degrade
rather than crash.

## Inspecting what the plugin sees

The plugin tracks an in-memory map of teammates. You can query it
out-of-band:

```bash
echo '{"method":"team.list","id":"diag","params":{}}' | \
    zellij pipe --name roger-rpc
```

This prints a JSON response listing every teammate the plugin
currently knows about, with `pane_id`, `name`, `command`, and
lifecycle flags (`exited`, `exit_code`). Useful for debugging when
the shim's view of reality and the plugin's diverge.

## Common operations

### Resume a teammate's session from another shell

Each Claude teammate writes its conversation state to a session
file under `~/.config/claude/sessions/` (path depends on your
Claude version). To resume one from a non-Zellij shell:

```bash
claude --resume <teammate-session-id>
```

That session id is in the teammate pane's startup output. This
works because teammates are real `claude` processes — not
sub-conversations of the lead — so the session state is durable
across attaches.

### Kill the lead → clean up teammates

When you exit the lead Claude session, the lead emits cleanup RPCs
(`team.kill` for each teammate). The plugin closes those panes
optimistically. If for any reason a teammate pane is still around
after the lead is gone (e.g. the lead was force-killed before
sending cleanup), close the pane manually in Zellij — the plugin's
`PaneClosed` handler removes the entry from its tracking map.

### Re-run a teammate's command

If a teammate's process exited and you want to re-run the same
command (without re-spawning the pane), use Zellij's
`<Ctrl-p>` then `Tab → Re-run command` keybinding. This emits a
`CommandPaneReRun` event that the plugin handles by clearing the
`exited` / `exit_code` flags on the entry. The pane id stays
stable across re-runs.

### Inspect plugin log

Plugin-side `eprintln!` lines go to Zellij's runtime log. On Linux:

```bash
tail -f /tmp/zellij-$(id -u)/zellij-log/zellij.log
```

On macOS / BSD the path differs — `zellij setup --check` shows the
cache and config dirs for your install. The log captures plugin
panics, permission denials, and our handlers' `eprintln!` lines
(unknown subcommand warnings, watchdog expiries, etc).

## Troubleshooting

See [`install.md`](install.md#troubleshooting) for env-setup
issues. The rest live here:

### Teammate spawns but immediately exits

The teammate's command (e.g. `claude --agent-id ...`) failed at
startup. Watch the pane scrollback. Common causes:
- Claude binary not on the teammate's `PATH` (the teammate inherits
  the parent shell's env, which may not match the lead Claude's
  env if you invoked Claude from a different shell).
- Stale session id reuse — Claude's agent-teams machinery picked
  an id that conflicts with a prior session file.

### `team.list` returns more panes than I see

You closed a teammate pane in Zellij but the plugin's state still
shows it. The plugin should have removed the entry via
`PaneClosed`, but if Zellij didn't emit that event (rare;
verified to happen only in old Zellij versions), the entry leaks.
For v0.1, restart the Zellij session to clear plugin state.

### Spawn hangs for ~10 seconds, then errors

The plugin's spawn watchdog (PR #57) is doing its job — the
teammate's `argv[0]` wasn't a valid binary, so Zellij never
emitted `CommandPaneOpened`, and the plugin gave up after the
TTL. Check the spawn args (`team.list` after the timeout fires
will be empty; the failure was logged to the plugin log).

### `send-keys` text arrives garbled

If your TmuxBackend is invoking `tmux send-keys` with shell
metacharacters (`'`, `"`, `$`, `\`) and they're showing up
literally in the teammate's PTY, the shim's `render_keys` is
working as designed — text tokens go through verbatim, no shell
quoting. Tmux behaves the same way; this is a TmuxBackend
question, not a shim question.

### A new `tmux` subcommand isn't being handled

Look in the plugin log for the catch-all warning:

```text
roger-shim: unknown tmux subcommand "<X>"; treating as no-op
```

If `<X>` is something Claude Code added in a recent version and it
needs real semantics rather than no-op, file an issue. The
existing handler shape in `shim/src/commands/` is small to extend.

### New teammate pane briefly shows a shell prompt

Expected. Real tmux's `new-session` / `new-window` / `split-window`
all create *empty* panes (running `$SHELL`); the follow-up
`send-keys` is what launches the teammate. roger-shim matches that
behavior — see [`shim.md`](shim.md#v01-limitations) for the
detailed rationale. The shell-prompt flash typically lasts
~50-200ms before `claude` exec's. If it's persisting longer, the
follow-up `send-keys` failed; check the plugin log for
`team.send` errors.

### Verifying Claude Code actually routes through TmuxBackend

The shim only fires when Claude shells out to `tmux <subcommand>`.
On platforms or Claude versions where Claude uses iTerm2, the
in-process backend, or a different mechanism, the shim is
bypassed entirely.

Quick check: with the shim activated, run a teammate-spawning
prompt in Claude and watch for `eprintln!` output from the shim
in Claude's host terminal (every `tmux` shell-out the shim sees
emits at least the "ZELLIJ_SESSION_NAME unset" path or its
no-op-warning path). If you see nothing, Claude isn't invoking
`tmux` for teammate ops; consult Claude Code's own settings for
forcing TmuxBackend.

## What's not covered yet (v0.1 limitations)

- **No multi-host orchestration.** Teammates can only live in the
  same Zellij session as the lead. Cross-host teams (e.g. a lead
  on your laptop, teammates on a build machine) aren't supported
  in v0.1. The trust model assumes a single-UID single-host
  posture; see [`trust-model.md`](trust-model.md).
- **No tab title decorations yet.** Claude state (working / idle /
  needs-input) is tracked internally but not surfaced as tab
  badges or pane border styles. That's #13 (Phase E).
- **No e2e test in CI.** Every PR is unit-tested, but real
  Zellij + plugin + shim + Claude integration is exercised
  manually per the workflow above. #11 captured the steps; a CI
  harness would require running Zellij headless which we haven't
  invested in.

See the [project roadmap](../ROADMAP.md) for what comes after.
