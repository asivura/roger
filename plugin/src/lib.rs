//! roger — a Zellij plugin for orchestrating Claude Code agent teams as
//! native panes.
//!
//! Named after Roger Penrose; see README.md for the full story. This
//! file is the scaffold: it registers the plugin, requests the
//! permissions it will eventually need, subscribes to the pane
//! lifecycle events that the RPC layer will react to, and stubs out
//! the `pipe()` entrypoint where the shim CLI will deliver
//! `team.spawn` / `team.send` / `team.kill` / `team.list` calls.
//!
//! Nothing actually orchestrates anything yet. Subsequent commits will
//! implement the RPC methods one at a time.

use std::collections::BTreeMap;
use zellij_tile::prelude::*;

#[derive(Default)]
struct State {
    // Future fields, intentionally empty for the scaffold:
    //   teammates: HashMap<AgentId, PaneId>
    //   teams: HashMap<TeamName, TeamConfig>
    //   inflight_rpcs: HashMap<PipeId, RpcKind>
}

register_plugin!(State);

impl ZellijPlugin for State {
    fn load(&mut self, _configuration: BTreeMap<String, String>) {
        // Permissions roger will eventually need. Requested up front so
        // the user only confirms once, even though most of these are
        // unused in the scaffold.
        request_permission(&[
            // Read the team config files, list panes, inspect state.
            PermissionType::ReadApplicationState,
            // Open / close / rename / focus panes, change layouts.
            PermissionType::ChangeApplicationState,
            // Spawn `claude` (and friends) into new panes.
            PermissionType::OpenTerminalsOrPlugins,
            // `run_command` escape hatch for reading
            // `~/.claude/teams/<team>/config.json` (the Wasm sandbox
            // does not expose arbitrary filesystem paths).
            PermissionType::RunCommands,
            // Send keystrokes / text into teammate panes (the inner
            // half of TmuxBackend's `send-keys` semantics).
            PermissionType::WriteToStdin,
            // Accept RPC over `zellij pipe`.
            PermissionType::ReadCliPipes,
            // TODO: `PermissionType::ReadPaneContents` was added in
            // zellij-tile 0.44; once we bump the dep, request it so we
            // can capture pane scrollback for the observability
            // surface (status, "what did this teammate just do"
            // hover, etc).
        ]);

        // Pane lifecycle events. The shim CLI tells us *what* to spawn;
        // these tell us when the resulting process has actually
        // started, exited (with what code), or been closed.
        subscribe(&[
            EventType::PaneUpdate,
            EventType::CommandPaneOpened,
            EventType::CommandPaneExited,
            EventType::CommandPaneReRun,
            EventType::PaneClosed,
        ]);

        // roger has no UI of its own; it lives entirely in the
        // permission-granted background.
        hide_self();
    }

    fn update(&mut self, event: Event) -> bool {
        match event {
            Event::CommandPaneOpened(pane_id, _ctx) => {
                eprintln!("[roger] CommandPaneOpened pane_id={}", pane_id);
            }
            Event::CommandPaneExited(pane_id, exit_code, _ctx) => {
                eprintln!(
                    "[roger] CommandPaneExited pane_id={} exit_code={:?}",
                    pane_id, exit_code
                );
            }
            Event::PaneClosed(pane_id) => {
                eprintln!("[roger] PaneClosed pane_id={:?}", pane_id);
            }
            _ => {}
        }
        // No re-render needed: the plugin is hidden.
        false
    }

    fn pipe(&mut self, pipe_message: PipeMessage) -> bool {
        // RPC entrypoint. The shim CLI invokes:
        //   zellij pipe --name roger-rpc --plugin file:~/.config/zellij/plugins/roger.wasm
        //
        // Planned methods (none implemented yet):
        //   team.spawn { name, cwd, argv, color? }       -> { pane_id }
        //   team.send  { pane_id, text }                 -> { ok }
        //   team.kill  { pane_id }                       -> { ok }
        //   team.list  {}                                -> { panes: [ ... ] }
        //
        // For now we just log the incoming pipe and return false
        // (don't re-render).
        eprintln!(
            "[roger] pipe source={:?} name={:?} payload_len={}",
            pipe_message.source,
            pipe_message.name,
            pipe_message.payload.as_ref().map(|p| p.len()).unwrap_or(0)
        );
        false
    }

    fn render(&mut self, _rows: usize, _cols: usize) {
        // Hidden plugin: nothing to render.
    }
}
