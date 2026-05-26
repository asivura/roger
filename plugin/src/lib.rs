//! roger — a Zellij plugin for orchestrating Claude Code agent teams as
//! native panes.
//!
//! Named after Roger Penrose; see README.md for the full story.
//!
//! This file is the plugin entrypoint. It registers the plugin, requests
//! the permissions the RPC layer needs, subscribes to pane lifecycle
//! events, and dispatches `zellij pipe` payloads to method handlers.
//!
//! Implemented methods:
//!   - `team.list` (PR #35)
//!   - `team.spawn` (PR #44)
//!   - `team.send` + `team.kill` (PR #50)
//!   - pane lifecycle wiring (this PR — #8): `CommandPaneExited`
//!     marks the teammate `exited: true` and records the exit code,
//!     `PaneClosed` removes the entry from `State::teammates` (idempotent
//!     w.r.t. `team.kill`'s optimistic removal), `CommandPaneReRun`
//!     clears the `exited` / `exit_code` fields so the teammate
//!     surfaces as live again.
//!
//! ## team.spawn: deferred-reply pattern
//!
//! `open_command_pane` in `zellij-tile 0.43.1` returns `()` — the
//! resulting pane id arrives asynchronously via the `CommandPaneOpened`
//! event. To honor the synchronous shim → plugin RPC (the shim's
//! `zellij pipe` call blocks until we reply), we:
//!
//! 1. In `pipe()`, parse the spawn request, generate a correlation
//!    token (we reuse `request.id`), insert a `PendingSpawn` into
//!    `State::pending_spawns`, then call `open_command_pane` with the
//!    token in its `Context` map.
//! 2. Return from `pipe()` WITHOUT replying. The CLI side stays
//!    blocked.
//! 3. When `CommandPaneOpened(pane_id, ctx)` fires in `update()`, we
//!    extract the token from `ctx`, look up the pending spawn, insert
//!    a `TeammatePaneInfo` into `State::teammates`, and call `reply()`
//!    — which unblocks the CLI.
//!
//! Limitation (deferred to a follow-up): no timeout. If
//! `CommandPaneOpened` never arrives (e.g. argv refers to a missing
//! binary), the shim hangs until its own client-side timeout fires.

use std::collections::{BTreeMap, HashMap};
use std::path::PathBuf;

use zellij_tile::prelude::*;

use roger_proto::{
    error_codes, KillParams, OkResult, Request, Response, SendParams, SpawnParams, SpawnResult,
    TeamListResult, TeammatePaneInfo, UNKNOWN_PANE_ID_MSG,
};

/// Key used in `CommandToRun`'s `Context` map to carry the per-spawn
/// correlation token from `pipe()` (where we open the pane) to
/// `update()` (where we get the resulting pane id and reply).
const SPAWN_TOKEN_KEY: &str = "roger.spawn_token";

/// How long a `team.spawn` can wait for `CommandPaneOpened` before the
/// watchdog gives up on it. Zellij does **not** emit
/// `CommandPaneOpened` when `spawn_terminal` returns
/// `Err(CommandNotFound)` (verified by the correctness reviewer on
/// PR #44 against `zellij-server/src/pty.rs`), so without a watchdog
/// the corresponding `PendingSpawn` leaks forever and the shim hangs
/// until its own client-side timeout. 10s is the same default the
/// shim uses for its read timeout.
const SPAWN_WATCHDOG_TTL_SECS: f64 = 10.0;

/// How often to wake up and sweep expired `pending_spawns` entries.
/// Slightly shorter than the TTL so an expired entry is found within
/// one tick after timeout. Trade-off: shorter = tighter timeout
/// reporting; longer = fewer wakeups. 5s gives ≤15s worst-case wait
/// before SPAWN_FAILED reaches the caller.
const SPAWN_WATCHDOG_TICK_SECS: f64 = 5.0;

#[derive(Default)]
struct State {
    /// Currently-tracked teammate panes, keyed by Zellij *terminal*
    /// pane id (`PaneId::Terminal(u32)` per zellij-utils 0.43.1; the
    /// `u32` is what arrives in `CommandPaneOpened`). The shim
    /// addresses subsequent `team.send` / `team.kill` calls by pane
    /// id (the value it received from `team.spawn`), so keying by
    /// pane id makes those lookups direct. Populated by the
    /// `CommandPaneOpened` handler after a `team.spawn` succeeds.
    ///
    /// **Invariant:** the only insertion site is
    /// `on_command_pane_opened`. `team.send` and `team.kill` rely on
    /// this to be safe in the face of Zellij pane-id reuse: if a
    /// terminal pane closes and Zellij later assigns the same numeric
    /// id to a non-Roger pane, that pane won't be in `teammates`, so
    /// the membership check rejects the operation. The lifecycle
    /// update paths (`on_command_pane_exited`, `on_pane_closed`,
    /// `on_command_pane_rerun`) likewise rely on this — they only
    /// mutate entries that exist, so a spoofed event for a pane id
    /// the plugin never inserted is a no-op. Don't add insertion
    /// sites elsewhere without revisiting the security review on
    /// PR #50.
    teammates: HashMap<u32, TeammatePaneInfo>,
    /// In-flight `team.spawn` calls awaiting `CommandPaneOpened`,
    /// keyed by a plugin-internal correlation token (see
    /// `next_spawn_token`). We do NOT reuse `request.id` as the key
    /// because the shim is allowed to repeat ids — colliding ids
    /// would silently drop the first caller's `PendingSpawn` and
    /// route the wrong response. (correctness reviewer, PR #44.)
    pending_spawns: HashMap<u64, PendingSpawn>,
    /// Monotonic counter for spawn correlation tokens. Wraps at
    /// `u64::MAX` which is fine — the universe ends before we hit it.
    next_spawn_token: u64,
    /// Cumulative seconds elapsed across all `Event::Timer` ticks the
    /// plugin has seen. The Wasm sandbox doesn't expose a
    /// monotonic clock (`std::time::Instant::now()` is unreliable on
    /// `wasm32-wasip1`), so we accumulate the `f64` payload of each
    /// `Event::Timer` as a proxy. The payload is the wall-clock
    /// elapsed time since `set_timeout` was called (verified by the
    /// zellij-api reviewer on PR #57 against
    /// `zellij-server/src/.../wasm_bridge.rs` and the protobuf
    /// definition) — close to but generally ≥ the requested
    /// duration due to scheduling jitter. Note: that payload
    /// round-trips through protobuf as `f32`, so sub-second tick
    /// intervals lose precision; not a concern at our 5s cadence
    /// but worth flagging if `SPAWN_WATCHDOG_TICK_SECS` is ever
    /// dropped to sub-second values. Used to age `pending_spawns`
    /// against `SPAWN_WATCHDOG_TTL_SECS`.
    watchdog_elapsed_secs: f64,
    /// `true` while a `set_timeout` is outstanding. **Load-bearing
    /// for correctness, not just efficiency** — each `set_timeout`
    /// call spawns an independent host-side task in
    /// `zellij-tile 0.43.1`, and there is no `clear_timeout` to
    /// cancel one. Without this guard, calling `set_timeout`
    /// multiple times in flight would deliver multiple
    /// `Event::Timer` events, which would double-count elapsed time
    /// against `watchdog_elapsed_secs` (since each event payload is
    /// "elapsed since that timer's set_timeout call", they overlap
    /// rather than tile). The guard ensures exactly one in-flight
    /// timer at a time. Cleared at the top of `on_watchdog_tick` so
    /// the next arm-request gets honored.
    watchdog_armed: bool,
}

/// Context the plugin remembers while a `team.spawn` is in flight,
/// so the `CommandPaneOpened` handler can finish the RPC.
struct PendingSpawn {
    pipe_id: String,
    request_id: String,
    agent_id: String,
    name: String,
    command: String,
    /// Value of `State::watchdog_elapsed_secs` at the moment this
    /// `PendingSpawn` was inserted. The watchdog expires the entry
    /// when `state.watchdog_elapsed_secs - created_at_secs >=
    /// SPAWN_WATCHDOG_TTL_SECS`.
    created_at_secs: f64,
}

/// What `handle_pipe` returns to `pipe()`.
///
/// Most methods reply immediately (`Reply`). `team.spawn` defers the
/// reply to the `CommandPaneOpened` event handler (`Deferred`); the
/// CLI side stays blocked until that handler calls `reply()`.
enum DispatchOutcome {
    Reply(Response),
    Deferred,
}

register_plugin!(State);

impl ZellijPlugin for State {
    fn load(&mut self, _configuration: BTreeMap<String, String>) {
        request_permission(&[
            PermissionType::ReadApplicationState,
            PermissionType::ChangeApplicationState,
            PermissionType::OpenTerminalsOrPlugins,
            PermissionType::RunCommands,
            PermissionType::WriteToStdin,
            PermissionType::ReadCliPipes,
            // TODO: `PermissionType::ReadPaneContents` was added in
            // zellij-tile 0.44; once we bump the dep (#13), request it
            // for the observability surface.
        ]);

        subscribe(&[
            EventType::CommandPaneOpened,
            EventType::CommandPaneExited,
            EventType::CommandPaneReRun,
            EventType::PaneClosed,
            // The spawn watchdog rearms `set_timeout` after each tick
            // while `pending_spawns` is non-empty; receiving the
            // resulting `Event::Timer` payload requires the
            // subscription.
            EventType::Timer,
        ]);

        hide_self();
    }

    fn update(&mut self, event: Event) -> bool {
        match event {
            Event::CommandPaneOpened(pane_id, ctx) => {
                self.on_command_pane_opened(pane_id, &ctx);
            }
            Event::CommandPaneExited(pane_id, exit_code, _ctx) => {
                self.on_command_pane_exited(pane_id, exit_code);
            }
            Event::PaneClosed(pane_id) => {
                self.on_pane_closed(pane_id);
            }
            Event::CommandPaneReRun(pane_id, _ctx) => {
                self.on_command_pane_rerun(pane_id);
            }
            Event::Timer(elapsed_secs) => {
                self.on_watchdog_tick(elapsed_secs);
            }
            _ => {}
        }
        false
    }

    fn pipe(&mut self, pipe_message: PipeMessage) -> bool {
        // Once we match `PipeSource::Cli`, the CLI is BLOCKED until we
        // reply. Every path below MUST either call `reply(...)` or
        // return `DispatchOutcome::Deferred` (in which case the event
        // handler is responsible for the reply).
        let pipe_id = match &pipe_message.source {
            PipeSource::Cli(id) => id.clone(),
            other => {
                eprintln!("[roger] non-CLI pipe ignored: {:?}", other);
                return false;
            }
        };

        match self.handle_pipe(&pipe_id, &pipe_message) {
            DispatchOutcome::Reply(response) => reply(&pipe_id, &response),
            DispatchOutcome::Deferred => {
                // Reply will be sent by the event handler when the
                // spawn completes. Do not touch the pipe here.
            }
        }
        false
    }

    fn render(&mut self, _rows: usize, _cols: usize) {
        // Hidden plugin: nothing to render.
    }
}

impl State {
    /// Deserialize `request.params` into the method-specific params
    /// struct, returning a ready-to-send `INVALID_PARAMS` `Response`
    /// on failure. The error message follows the `"invalid {method}
    /// params: {serde error}"` shape every handler used before this
    /// helper landed. (#51 — followups from PR #50 review.)
    fn parse_params<T: serde::de::DeserializeOwned>(
        request: &Request,
        method: &str,
    ) -> Result<T, Response> {
        serde_json::from_value(request.params.clone()).map_err(|e| {
            Response::err(
                &request.id,
                error_codes::INVALID_PARAMS,
                format!("invalid {} params: {}", method, e),
            )
        })
    }

    /// Two-stage parse so the `id` survives a wrong-shape body and we
    /// can emit the right error code (`PARSE_ERROR` for non-JSON,
    /// `INVALID_REQUEST` for JSON-but-wrong-shape). PR #35 review
    /// (correctness + error-handling) called this out as Important.
    fn handle_pipe(&mut self, pipe_id: &str, pipe_message: &PipeMessage) -> DispatchOutcome {
        let payload = match pipe_message.payload.as_deref() {
            Some(p) if !p.is_empty() => p,
            _ => {
                return DispatchOutcome::Reply(Response::err(
                    "",
                    error_codes::PARSE_ERROR,
                    "empty payload",
                ));
            }
        };

        let raw: serde_json::Value = match serde_json::from_str(payload) {
            Ok(v) => v,
            Err(e) => {
                return DispatchOutcome::Reply(Response::err(
                    "",
                    error_codes::PARSE_ERROR,
                    format!("invalid JSON: {}", e),
                ));
            }
        };

        let salvaged_id = raw
            .get("id")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();

        let request: Request = match serde_json::from_value(raw) {
            Ok(r) => r,
            Err(e) => {
                return DispatchOutcome::Reply(Response::err(
                    &salvaged_id,
                    error_codes::INVALID_REQUEST,
                    format!("invalid request shape: {}", e),
                ));
            }
        };

        self.dispatch(pipe_id, &request)
    }

    fn dispatch(&mut self, pipe_id: &str, request: &Request) -> DispatchOutcome {
        match request.method.as_str() {
            "team.list" => DispatchOutcome::Reply(self.handle_team_list(request)),
            "team.spawn" => self.handle_team_spawn(pipe_id, request),
            "team.send" => DispatchOutcome::Reply(self.handle_team_send(request)),
            "team.kill" => DispatchOutcome::Reply(self.handle_team_kill(request)),
            other => DispatchOutcome::Reply(Response::err(
                &request.id,
                error_codes::METHOD_NOT_FOUND,
                format!("method not found: {}", other),
            )),
        }
    }

    /// `team.list` — synchronous, just serializes the state map.
    fn handle_team_list(&self, request: &Request) -> Response {
        let panes: Vec<TeammatePaneInfo> = self.teammates.values().cloned().collect();
        let result = TeamListResult { panes };
        // `TeamListResult` is a `Vec` of owned-`String` structs;
        // serde_json cannot fail to serialize this shape. (#52 —
        // dropped the dead INTERNAL_ERROR branch.)
        let value = serde_json::to_value(&result).expect("TeamListResult serializes infallibly");
        Response::ok(&request.id, value)
    }

    /// `team.spawn` — async via the correlation-token pattern. See the
    /// module doc comment for the full flow.
    fn handle_team_spawn(&mut self, pipe_id: &str, request: &Request) -> DispatchOutcome {
        let params: SpawnParams = match Self::parse_params(request, "team.spawn") {
            Ok(p) => p,
            Err(resp) => return DispatchOutcome::Reply(resp),
        };

        if params.argv.is_empty() {
            return DispatchOutcome::Reply(Response::err(
                &request.id,
                error_codes::SPAWN_FAILED,
                "team.spawn: argv is empty",
            ));
        }

        // Mint a fresh internal token. Do NOT reuse `request.id` —
        // the shim is allowed to repeat ids and colliding ids would
        // silently drop a caller's PendingSpawn (correctness review,
        // PR #44). The protocol id is still echoed back via
        // PendingSpawn::request_id.
        let token = self.next_spawn_token;
        self.next_spawn_token = self.next_spawn_token.wrapping_add(1);
        let command_display = params.argv.join(" ");

        let cmd = CommandToRun {
            path: PathBuf::from(&params.argv[0]),
            args: params.argv[1..].to_vec(),
            cwd: Some(PathBuf::from(&params.cwd)),
        };

        self.pending_spawns.insert(
            token,
            PendingSpawn {
                pipe_id: pipe_id.to_string(),
                request_id: request.id.clone(),
                agent_id: params.agent_id,
                name: params.name,
                command: command_display,
                created_at_secs: self.watchdog_elapsed_secs,
            },
        );
        self.arm_watchdog();

        let mut ctx = BTreeMap::new();
        ctx.insert(SPAWN_TOKEN_KEY.to_string(), token.to_string());
        open_command_pane(cmd, ctx);

        // Reply is deferred to `on_command_pane_opened` in update().
        // If the shim closes the pipe before then (timeout, SIGINT),
        // the eventual cli_pipe_output/unblock_cli_pipe_input calls
        // silently no-op on the host side — verified safe in
        // zellij-tile 0.43.1, no panic, no abort (error-handling
        // reviewer, PR #44).
        //
        // If Zellij never emits `CommandPaneOpened` (e.g. the binary
        // at `argv[0]` doesn't exist; verified against
        // `zellij-server/src/pty.rs`'s `spawn_terminal` path), the
        // watchdog armed above will sweep this `PendingSpawn` after
        // `SPAWN_WATCHDOG_TTL_SECS` and reply `SPAWN_FAILED`.
        DispatchOutcome::Deferred
    }

    /// Called from `update()` when a Zellij pane finishes opening.
    /// Looks up the pending spawn by correlation token, inserts the
    /// teammate into `State::teammates`, and replies on the CLI pipe.
    fn on_command_pane_opened(&mut self, pane_id: u32, ctx: &BTreeMap<String, String>) {
        let token_str = match ctx.get(SPAWN_TOKEN_KEY) {
            Some(t) => t,
            None => {
                // A `CommandPaneOpened` we didn't initiate (e.g. the
                // user opened a pane manually). Nothing to do.
                eprintln!(
                    "[roger] CommandPaneOpened pane_id={} (no roger token; ignored)",
                    pane_id
                );
                return;
            }
        };
        let token: u64 = match token_str.parse() {
            Ok(t) => t,
            Err(_) => {
                eprintln!(
                    "[roger] CommandPaneOpened pane_id={} unparseable token={:?}; ignored",
                    pane_id, token_str
                );
                return;
            }
        };

        let pending = match self.pending_spawns.remove(&token) {
            Some(p) => p,
            None => {
                // Token present but we don't remember this spawn.
                // Possible race / restart; log and drop.
                eprintln!(
                    "[roger] CommandPaneOpened pane_id={} token={} not in pending_spawns",
                    pane_id, token
                );
                return;
            }
        };

        self.teammates.insert(
            pane_id,
            TeammatePaneInfo {
                agent_id: pending.agent_id,
                pane_id,
                name: pending.name,
                command: Some(pending.command),
                exited: false,
                exit_code: None,
            },
        );

        // `SpawnResult { pane_id: u32 }` cannot fail to serialize.
        // (#52 — dropped the dead INTERNAL_ERROR branch.)
        let result = serde_json::to_value(SpawnResult { pane_id })
            .expect("SpawnResult serializes infallibly");
        reply(&pending.pipe_id, &Response::ok(&pending.request_id, result));
    }

    /// `Event::CommandPaneExited` handler (#8).
    ///
    /// Mark the matching teammate as exited and record the exit code.
    /// We do **not** remove from `State::teammates` here — the pane
    /// (and its scrollback) survives until the operator closes it
    /// (which produces `PaneClosed`) or re-runs it
    /// (`CommandPaneReRun`, which clears the exited state). Keeping
    /// the entry lets `team.list` continue to surface the dead
    /// teammate, so the operator can inspect the exit status.
    ///
    /// An unknown `pane_id` is a normal case, not an error: if
    /// `team.kill` already removed the entry, the eventual
    /// `CommandPaneExited` finds nothing to update. Logged and
    /// dropped.
    fn on_command_pane_exited(&mut self, pane_id: u32, exit_code: Option<i32>) {
        match self.teammates.get_mut(&pane_id) {
            Some(t) => {
                t.exited = true;
                // Don't clobber a previously-recorded exit code if
                // Zellij re-emits the event with `None` (defensive;
                // correctness reviewer on PR #54).
                t.exit_code = exit_code.or(t.exit_code);
                debug_assert!(
                    t.exited || t.exit_code.is_none(),
                    "TeammatePaneInfo invariant: exit_code is Some only when exited"
                );
            }
            None => {
                eprintln!(
                    "[roger] CommandPaneExited pane_id={} exit_code={:?} (not tracked; ignored)",
                    pane_id, exit_code
                );
            }
        }
    }

    /// `Event::PaneClosed` handler (#8).
    ///
    /// Remove the teammate entry. Idempotent: if `team.kill` already
    /// removed the pane optimistically (PR #50), this is a no-op.
    /// Only `PaneId::Terminal` ids can match `State::teammates`
    /// because the only insertion site is `on_command_pane_opened`,
    /// which receives a `u32` terminal pane id (validated by the
    /// zellij-api reviewer on PR #50). Plugin-pane closures are
    /// logged and ignored.
    fn on_pane_closed(&mut self, pane_id: PaneId) {
        match pane_id {
            PaneId::Terminal(id) => {
                // Silent. The not-tracked case is the *happy path* for
                // a `team.kill` round-trip (PR #50's optimistic-remove
                // already cleared the entry, so the eventual
                // `PaneClosed` finds nothing to do). Logging here
                // would emit a "not tracked" line on every successful
                // kill — pure noise. (correctness reviewer, PR #54.)
                let _ = self.teammates.remove(&id);
            }
            PaneId::Plugin(id) => {
                eprintln!(
                    "[roger] PaneClosed pane_id=Plugin({}); we never track plugin panes",
                    id
                );
            }
        }
    }

    /// `Event::CommandPaneReRun` handler (#8).
    ///
    /// Operator re-ran an exited teammate's command. Clear the exited
    /// state so `team.list` surfaces it as live again. The pane id is
    /// stable across re-runs in zellij-tile 0.43.1, so no remap is
    /// needed — we just flip the flags back.
    fn on_command_pane_rerun(&mut self, pane_id: u32) {
        match self.teammates.get_mut(&pane_id) {
            Some(t) => {
                t.exited = false;
                t.exit_code = None;
                debug_assert!(
                    t.exited || t.exit_code.is_none(),
                    "TeammatePaneInfo invariant: exit_code is Some only when exited"
                );
            }
            None => {
                eprintln!(
                    "[roger] CommandPaneReRun pane_id={} (not tracked; ignored)",
                    pane_id
                );
            }
        }
    }

    /// `team.send` — write text into a tracked teammate pane's PTY.
    ///
    /// Synchronous at the JSON-RPC layer: we reply as soon as the
    /// write is dispatched to Zellij. The underlying
    /// `write_chars_to_pane_id` is fire-and-forget host-side
    /// (`zellij-tile 0.43.1` returns `()` with no acknowledgement),
    /// so `{ "ok": true }` means *the plugin handed the bytes to
    /// Zellij*, not *the bytes reached the PTY*. The most plausible
    /// failure mode is a write swallowed by a pane that exited
    /// between the membership check and the host call — harmless,
    /// since #8's `PaneClosed` cleanup will remove the entry shortly.
    fn handle_team_send(&self, request: &Request) -> Response {
        let params: SendParams = match Self::parse_params(request, "team.send") {
            Ok(p) => p,
            Err(resp) => return resp,
        };

        if !self.teammates.contains_key(&params.pane_id) {
            // Don't echo `params.pane_id` in the message — the caller
            // already knows the value they sent. Keeping the response
            // value-free removes a behavioral oracle that a future
            // authz layer would otherwise inherit by accident
            // (security review, PR #50).
            return Response::err(
                &request.id,
                error_codes::INVALID_PARAMS,
                UNKNOWN_PANE_ID_MSG.to_string(),
            );
        }

        write_chars_to_pane_id(&params.text, PaneId::Terminal(params.pane_id));

        // `OkResult { ok: bool }` cannot fail to serialize. (#52.)
        let value =
            serde_json::to_value(OkResult { ok: true }).expect("OkResult serializes infallibly");
        Response::ok(&request.id, value)
    }

    /// `team.kill` — close a tracked teammate pane and remove it
    /// from `State::teammates`. Optimistic removal: we drop the
    /// entry immediately rather than waiting for `PaneClosed`, since
    /// the shim explicitly asked for the kill and the call is
    /// fire-and-forget. The eventual `PaneClosed` event will find no
    /// matching entry and harmlessly no-op (#8 wiring).
    fn handle_team_kill(&mut self, request: &Request) -> Response {
        let params: KillParams = match Self::parse_params(request, "team.kill") {
            Ok(p) => p,
            Err(resp) => return resp,
        };

        if self.teammates.remove(&params.pane_id).is_none() {
            // Same rationale as `handle_team_send`: don't echo the
            // caller-supplied pane id back (security review, PR #50).
            return Response::err(
                &request.id,
                error_codes::INVALID_PARAMS,
                UNKNOWN_PANE_ID_MSG.to_string(),
            );
        }

        close_pane_with_id(PaneId::Terminal(params.pane_id));

        // `OkResult { ok: bool }` cannot fail to serialize. (#52.)
        let value =
            serde_json::to_value(OkResult { ok: true }).expect("OkResult serializes infallibly");
        Response::ok(&request.id, value)
    }

    /// Schedule the next watchdog `Event::Timer` if one isn't already
    /// in flight. Cheap to call repeatedly: the `watchdog_armed` flag
    /// guards against stacking duplicate timers when multiple spawns
    /// pile up. The flag is cleared in `on_watchdog_tick` so the next
    /// arm-request gets honored. (#45.)
    fn arm_watchdog(&mut self) {
        if self.watchdog_armed {
            return;
        }
        set_timeout(SPAWN_WATCHDOG_TICK_SECS);
        self.watchdog_armed = true;
    }

    /// Sweep `pending_spawns` for entries that have been waiting
    /// longer than `SPAWN_WATCHDOG_TTL_SECS`. For each, send back a
    /// `SPAWN_FAILED` reply and remove the entry. Rearm the watchdog
    /// if any spawns remain pending so the next sweep happens
    /// `SPAWN_WATCHDOG_TICK_SECS` from now. (#45.)
    fn on_watchdog_tick(&mut self, elapsed_secs: f64) {
        self.watchdog_armed = false;
        self.watchdog_elapsed_secs += elapsed_secs;

        let now = self.watchdog_elapsed_secs;
        let expired: Vec<u64> = self
            .pending_spawns
            .iter()
            .filter(|(_, p)| now - p.created_at_secs >= SPAWN_WATCHDOG_TTL_SECS)
            .map(|(t, _)| *t)
            .collect();

        for token in expired {
            // `remove(&token)` returns `Some` here. We just collected
            // these tokens from the same map. The Wasm plugin sandbox
            // is single-threaded and runs one event callback at a
            // time — no other code path can mutate `pending_spawns`
            // between the `filter` collection above and this remove
            // (so `&mut self` isn't what makes this safe; the
            // sandbox's no-reentrancy guarantee is).
            let pending = self
                .pending_spawns
                .remove(&token)
                .expect("expired token was in pending_spawns this iteration");
            reply(
                &pending.pipe_id,
                &Response::err(
                    &pending.request_id,
                    error_codes::SPAWN_FAILED,
                    "team.spawn: timed out waiting for CommandPaneOpened",
                ),
            );
            eprintln!(
                "[roger] spawn watchdog: expired pending_spawn token={} agent_id={:?} (no CommandPaneOpened within {}s)",
                token, pending.agent_id, SPAWN_WATCHDOG_TTL_SECS
            );
        }

        // Keep ticking while spawns remain pending. If they all
        // resolved (or were just expired), let the watchdog sleep
        // until the next `handle_team_spawn` rearms it.
        if !self.pending_spawns.is_empty() {
            self.arm_watchdog();
        }
    }
}

/// Write a JSON response to the CLI pipe and unblock the caller.
///
/// The fallback path (if `serde_json::to_string(response)` itself
/// fails) builds the error JSON by hand, escaping the inner error
/// message through `serde_json` so the fallback is *always* valid
/// JSON (PR #35 review).
fn reply(pipe_id: &str, response: &Response) {
    let body = serde_json::to_string(response).unwrap_or_else(|e| {
        let safe_message = serde_json::to_string(&format!("failed to serialize response: {}", e))
            .unwrap_or_else(|_| "\"internal error\"".to_string());
        format!(
            r#"{{"id":"","error":{{"code":{},"message":{}}}}}"#,
            error_codes::INTERNAL_ERROR,
            safe_message
        )
    });
    cli_pipe_output(pipe_id, &body);
    unblock_cli_pipe_input(pipe_id);
}
