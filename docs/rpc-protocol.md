# RPC protocol (plugin ↔ shim)

This document describes the JSON-RPC-style protocol that the
`roger-shim` CLI and the `roger` Wasm plugin speak over Zellij CLI
pipes (`zellij pipe`).

## Transport

The shim invokes:

```bash
zellij pipe --name roger-rpc --plugin file:~/.config/zellij/plugins/roger.wasm
```

with the request body on stdin. The pipe blocks until the plugin
replies via `cli_pipe_output` + `unblock_cli_pipe_input`. From the
shim's perspective the call is synchronous: write a request, read a
response.

The `--name` value is treated as the pipe identifier by zellij-tile
0.43.1's reply API. We default to `roger-rpc`. Custom names are
allowed but the plugin doesn't care which name the shim picked.

## Request shape

```json
{
  "method": "team.list",
  "id": "f3a1...",
  "params": {}
}
```

- `method` (string, required) — the dotted method name (see below).
- `id` (string, required) — a caller-chosen correlation token,
  typically a UUID. Echoed verbatim in the response so the shim can
  correlate responses with in-flight requests.
- `params` (object, optional) — method-specific arguments. For
  `team.list` it's an empty object (or omitted).

## Response shape

Success:

```json
{
  "id": "f3a1...",
  "result": {
    "panes": [
      {
        "agent_id": "researcher@my-team",
        "pane_id": 17,
        "name": "researcher",
        "command": "claude --agent-id researcher@my-team ...",
        "exited": false
      },
      {
        "agent_id": "linter@my-team",
        "pane_id": 18,
        "name": "linter",
        "exited": true,
        "exit_code": 0
      }
    ]
  }
}
```

`exit_code` is omitted from the JSON when no exit code is known
(running teammate, or a teammate that exited without Zellij reporting
a code). Once set, it remains attached to the entry until either
`team.kill` / a `PaneClosed` event removes the entry, or the operator
re-runs the command (`CommandPaneReRun` clears the field).

Error:

```json
{
  "id": "f3a1...",
  "error": {
    "code": -32601,
    "message": "method not found: team.spaaaawn"
  }
}
```

Exactly one of `result` or `error` is present. The plugin never sets
both, and never omits both.

## Methods

### `team.list`

Returns the panes currently tracked by `roger`.

**Params:** none (or `{}`).

**Result:**

```json
{
  "panes": [
    {
      "agent_id": "<name>@<team>",
      "pane_id": <u32>,
      "name": "<human-name>",
      "command": "<optional spawn argv joined>",
      "exited": false
    }
  ]
}
```

The list is empty when no teammates have been spawned. That's a
valid response, not an error.

**Lifecycle field semantics (post-#54):**

| Field | Set by | Cleared by |
|---|---|---|
| `exited` | `Event::CommandPaneExited` (true) | `team.kill` / `PaneClosed` (removes entry); `CommandPaneReRun` (back to false) |
| `exit_code` | `Event::CommandPaneExited` (the reported code) | `team.kill` / `PaneClosed` (removes entry); `CommandPaneReRun` (back to None — omitted on wire) |

A teammate whose underlying command exited is **kept** in the map so
`team.list` can surface its post-mortem status. Only `team.kill` and
the Zellij `PaneClosed` event remove the entry.

### `team.spawn`

Spawns a teammate as a new Zellij pane and tracks it in
`State::teammates`. Returns the new pane's id so the shim can address
subsequent `team.send` / `team.kill` calls to it.

**Params:**

```json
{
  "agent_id": "researcher@my-team",
  "name": "researcher",
  "cwd": "/home/ubuntu/some/dir",
  "argv": ["claude", "--agent-id", "researcher@my-team", "--prompt", "..."],
  "color": "blue"
}
```

- `agent_id` (string, required) — the unique identifier the shim uses
  to address the teammate from Claude Code's bookkeeping. Echoed
  verbatim in `team.list`.
- `name` (string, required) — human-readable label for the pane.
- `cwd` (string, required) — working directory for the spawned process.
- `argv` (array of strings, required) — the command + args to run.
  Must be non-empty; `argv[0]` is the executable path.
- `color` (string, optional) — pane border color hint. Recognized
  values match Zellij's named colors. Defaults to no override.

**Result:**

```json
{ "pane_id": 17 }
```

**Errors:**

- `INVALID_PARAMS` (-32602) — params object doesn't match the
  expected shape (missing or wrong-typed field).
- `SPAWN_FAILED` (-32001) — `argv` was empty so there's no command
  to run. Roger-specific code in the JSON-RPC 2.0 server-error range.

**Async note:** internally the plugin's reply to `team.spawn` is
deferred to the Zellij `CommandPaneOpened` event (the pane id arrives
asynchronously via that callback). From the shim's perspective the
RPC is still synchronous — its `zellij pipe` call blocks until the
plugin sends the reply.

**Watchdog (post-#57):** the plugin enforces an internal 10-second
TTL on every in-flight `team.spawn`. If `CommandPaneOpened` doesn't
arrive within that window, the plugin replies `SPAWN_FAILED` with
message `"team.spawn: timed out waiting for CommandPaneOpened"` and
discards the pending entry. The TTL is configured via
`SPAWN_WATCHDOG_TTL_SECS` in `plugin/src/lib.rs`. The shim's
client-side read timeout still applies as the outer bound, but for
the common failure modes (missing binary, etc.) the plugin's
watchdog is what surfaces the error.

**Behavior when `argv[0]` is missing:** verified against
`zellij-server/src/pty.rs` — Zellij does **not** emit
`CommandPaneOpened` when `spawn_terminal` returns
`Err(CommandNotFound)`. The watchdog described above catches this
case: `SPAWN_FAILED` is returned after ~10s instead of the request
hanging until the shim's client-side timeout fires.

### `team.send`

Writes `text` into the PTY of a tracked teammate pane (the inner
half of TmuxBackend's `send-keys` semantics). Synchronous on the
RPC; the underlying `write_chars_to_pane_id` is fire-and-forget on
the Zellij side.

**Params:** `{ pane_id: u32, text: string }`. Both required.

**Result:** `{ ok: true }`.

**Result semantics:** `{ ok: true }` means *the plugin dispatched
the write to Zellij*, not *the bytes reached the PTY*. If the pane
exits between the plugin's membership check and Zellij's host-side
write, the bytes are silently dropped (Zellij does not surface a
host-side error to the plugin). Callers that need delivery
confirmation must read pane contents separately.

**Trust contract:** `text` is delivered to the PTY as-is — ANSI
CSI/OSC sequences, control characters, and `\r` (which causes a
shell to execute the buffered line) all pass through unchanged.
This matches `tmux send-keys` and is fine while the sole producer
is `roger-shim` relaying from Claude Code's own TmuxBackend. Do not
route untrusted text (e.g. another teammate's stdout) through this
method without sanitization — the same code path then becomes a
terminal-injection sink (OSC 52 clipboard writes, OSC 8 hyperlinks,
title spoof, cursor-position queries echoed back).

**Errors:**
- `INVALID_PARAMS` (-32602) — params don't match expected shape, OR
  `pane_id` isn't in `State::teammates` (unknown / dead pane). The
  error message is value-free (`"unknown pane_id"` rather than
  `"unknown pane_id: 42"`) — the caller already knows the value
  they sent, and keeping the response value-free removes a
  behavioral oracle that a future authz layer would otherwise
  inherit.

### `team.kill`

Closes a tracked teammate pane and removes it from `State::teammates`
optimistically (without waiting for the `PaneClosed` event). The
shim explicitly asked for the kill; the eventual `PaneClosed` will
find no matching entry and harmlessly no-op.

**Params:** `{ pane_id: u32 }`. Required.

**Result:** `{ ok: true }`.

**Errors:**
- `INVALID_PARAMS` (-32602) — params don't match, OR `pane_id` isn't
  in `State::teammates`. Same value-free message as `team.send`.

## Error codes

Mirrors the [JSON-RPC 2.0 reserved range](https://www.jsonrpc.org/specification#error_object):

| Code | Name | Meaning |
|---|---|---|
| `-32700` | `PARSE_ERROR` | The pipe payload was empty or not valid JSON. |
| `-32600` | `INVALID_REQUEST` | The payload parsed as JSON but didn't match the request shape. |
| `-32601` | `METHOD_NOT_FOUND` | The `method` string doesn't correspond to a registered handler. |
| `-32602` | `INVALID_PARAMS` | The `params` object didn't match the method's expected shape. |
| `-32603` | `INTERNAL_ERROR` | The plugin's handler hit an internal failure (e.g. result serialization). |

The shim should retry on `-32603` once (server-side transient
failure), and surface other error codes as fatal to its caller.

## Concurrency

Each `zellij pipe` call gets its own pipe identifier. The plugin
processes one pipe message per `pipe()` callback. Multiple shim
invocations are processed sequentially by the plugin; the CLI side
blocks on each. This is acceptable at the cadence Claude Code's
TmuxBackend generates calls (low single-digit per second at most).

## Non-CLI pipe sources

The plugin only responds to `PipeSource::Cli`. Plugin-to-plugin
(`PipeSource::Plugin`) and keybind-triggered (`PipeSource::Keybind`)
pipes are logged and ignored. The shim path is the only path that
needs to work for v0.1.

## Trust model

The protocol has no authentication, signing, or rate limiting. Any
local process at the same UID as the Zellij session can call every
method. See [`trust-model.md`](trust-model.md) for the full
deployment-assumptions document — required reading before deploying
roger anywhere shared-UID untrusted code might run.
