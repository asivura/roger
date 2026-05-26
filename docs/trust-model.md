# Trust model

This document describes the assumptions roger v0.1 makes about its
deployment environment and the producers / consumers of its RPC
protocol.

## What roger trusts

The plugin trusts that the producer of every `zellij pipe` request is
a process running as the same UID as the Zellij session itself, and
running on the same host. There is no authentication, no signing, no
caller identity check, and no rate limit.

The wire protocol (documented in [`rpc-protocol.md`](rpc-protocol.md))
defines no equivalent of an auth token or session handshake. Every
caller that can reach the pipe at the OS level is treated as fully
trusted to spawn teammates, write text into their PTYs, list tracked
panes, and close them.

This matches `tmux`'s posture: any process at the same UID can attach
to the session and send keystrokes. `roger` inherits that posture
because the design goal of v0.1 is functional parity with the
TmuxBackend it replaces, not a tighter security boundary.

## What that implies

The UID boundary is **load-bearing**. A v0.1 roger deployment is safe
on a single-user dev box (the intended deployment) and unsafe on any
system where:

- multiple users share the same UID;
- untrusted code runs at the same UID (e.g. a sandboxed plugin host
  that hasn't dropped privileges);
- the operator has explicitly granted UID-level access to a remote
  agent or session-sharing tool.

If you deploy roger somewhere those assumptions don't hold, treat
every RPC method as equivalent to `tmux send-keys --target <session>`
called from an arbitrary local process at the same UID. The blast
radius is "anything Claude Code (or any other teammate process) can
do".

## `team.send`'s extra surface

`team.send` writes `text` to a teammate's PTY verbatim. Because
terminals interpret ANSI CSI/OSC sequences, control characters, and
`\r` (which submits a shell line), the trust contract on `text`
matters separately from the trust contract on the caller.

The current expected producer is `roger-shim`, which only relays text
that Claude Code's `TmuxBackend` was going to emit anyway. That stays
fine.

**Do not** route arbitrary text from another source — especially
another teammate's stdout — through `team.send` without sanitizing
first. The same code path then becomes a terminal-injection sink: OSC
52 clipboard writes, OSC 8 hyperlinks, title spoofs, cursor-position
queries echoed back, terminal-feature-test races, etc. See PR #50's
security review (and the `SendParams` doc-comment in
`proto/src/lib.rs`) for the canonical statement of this contract.

## What is *not* a trust boundary

The plugin is sandboxed by Zellij's Wasm runtime. That sandbox
protects Zellij from the plugin — not the plugin from its callers.
It's the same direction of trust as any sandboxed extension: the host
is defended, the extension isn't.

`State::teammates` insertion is gated to a single site
(`on_command_pane_opened`, only reachable after the plugin itself
called `open_command_pane`). That gate is also load-bearing — see
the `State::teammates` doc-comment in `plugin/src/lib.rs` — but it
relies on the producer's trust to begin with. A trusted caller can
still legitimately cause arbitrary teammate spawns; that's the
feature, not a bug.

## What's out of scope (and tracked separately)

v0.1 explicitly does not implement:

- **Token / handshake-based authn** on the pipe. The UID boundary is
  considered sufficient for the v0.1 deployment target.
- **Per-call MAC / signing**. No multi-party threat model exists yet.
- **Privilege separation between teammate spawns**. All teammates run
  with the same authority as the plugin itself; if you need
  per-teammate scoping, that's a future feature, not a defect.
- **Audit log of RPC calls**. Plugin emits `eprintln!` lines to the
  Zellij plugin log on certain paths (unknown pane id, malformed
  spawn token, etc.), but those are diagnostic, not auditable.

If your threat model requires any of the above, roger v0.1 is the
wrong tool. Filing a Phase E+ issue with the specific requirement is
the right next step.

## Updating this document

This document is normative for v0.1. If a security review or PR adds
a new trust contract (e.g. PR #50's `SendParams` trust contract), the
short version belongs in the source code's doc-comment for
discoverability, and the long version belongs here for context.
