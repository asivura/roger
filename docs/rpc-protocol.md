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
      }
    ]
  }
}
```

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
plugin sends the reply. The shim's read timeout is the only thing
that bounds the wait if the spawn never completes; the plugin has
no internal timeout in v0.1 (tracked as a follow-up).

**Behavior when `argv[0]` is missing:** verified against
`zellij-server/src/pty.rs` — Zellij does **not** emit
`CommandPaneOpened` when `spawn_terminal` returns
`Err(CommandNotFound)`. So a `team.spawn` for a non-existent binary
will hang on the shim side until its read timeout fires; the
`PendingSpawn` entry leaks in the plugin's memory. The watchdog
follow-up addresses both.

### `team.send` *(planned, #7)*

Writes text into a teammate pane's PTY (the `send-keys` equivalent).

**Params:** `{ pane_id, text }`. **Result:** `{ ok: true }`.

### `team.kill` *(planned, #7)*

Closes a teammate pane and removes it from the state map.

**Params:** `{ pane_id }`. **Result:** `{ ok: true }`.

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
