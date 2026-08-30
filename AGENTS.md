# AGENTS.md — synx development guide

Working rules and architecture for anyone (human or AI) contributing to synx.

## What this is

Fast, real-time bidirectional file sync over SSH. One binary, two roles:
the client (`synx LOCAL REMOTE`) spawns itself on the remote over SSH
(`synx --agent PATH`) and they speak a framed binary protocol over the
SSH pipe. No daemons, no new auth — the user's existing `ssh` setup is
the transport.

## Architecture

```
client (local host)                          agent (remote host)
┌─────────────┐   ssh pipe (stdin/stdout)   ┌─────────────┐
│ sync.rs     │ ◀── framed protocol ──▶     │ agent.rs    │
│ cli.rs      │     protocol.rs             │             │
│ transport.rs│                             │             │
└──────┬──────┘                             └──────┬──────┘
       │            both sides share               │
       └──────────▶ peer.rs (fs ops, live loop, git gate,
                    chunked transfer, delta sync, suppression)
```

- `main.rs` — entrypoint; routes `--agent` to the agent, else to the client.
- `cli.rs` — clap definitions; `ClientArgs` is the client-side config struct.
- `transport.rs` — parses `[user@]host:/path`, builds/spawns the ssh command.
- `protocol.rs` — `Message` enum + framing: 4-byte BE length, 1 flag byte,
  postcard payload, optional zstd (flag bit) above a size threshold. Every
  decoded message is path-validated at this boundary.
- `sync.rs` — client orchestration: handshake, manifest exchange, plan &
  apply the initial diff, then the live loop. Owns the reconnect loop and
  the fatal-vs-transient error classification (`is_fatal`).
- `agent.rs` — remote-side counterpart: handshake, walk, manifest, applies
  client operations, forwards its own filesystem events.
- `peer.rs` — everything shared: safe fs mutations, chunked transfer,
  fast_rsync delta sync, the bidirectional live loop, event coalescing,
  echo suppression, and the git gate (pauses `.git/` traffic while git is
  mid-operation — rebase, merge, etc.).
- `walker.rs` — parallel manifest walk (blake3 hashing, all cores).
- `cache.rs` — persistent (size, mtime) → hash cache; re-runs skip re-hashing.
- `baseline.rs` — persisted converged manifest; the common ancestor of the
  three-way diff that distinguishes "deleted here" from "created there".
- `ignores.rs` — per-directory `.gitignore` / `.synxignore` matcher stack.
- `watcher.rs` — fs events via `notify` (FSEvents / inotify), debounced.
- `paths.rs` — `resolve_beneath`: every peer-requested path is confined to
  the sync root. No mutation escapes the root, ever.
- `ui.rs` — terminal output (colors, banner, progress lines).

Sync semantics worth knowing: `.git/` **is synced by design** (the gate only
pauses it during active git operations). At handshake, both sides report
their root's normalized git remotes; if both roots are identifiable repos
sharing zero remotes, the client refuses (`--allow-repo-mismatch` overrides).

## Hard rules

1. **No inline tests.** Unit tests live in `src/<module>_tests.rs`, wired
   from the module file with:

   ```rust
   #[cfg(test)]
   #[path = "<module>_tests.rs"]
   mod tests;
   ```

   Test-only *helpers* (e.g. `Baseline::from_entries`) may stay in the
   module behind `#[cfg(test)]`; `#[test]` functions never do.

2. **Verification runs on the dev server, never locally.** Local edits
   auto-sync to the mirror at `dev:/home/box/work/muvon/synx`. Check
   results there:

   ```bash
   ssh dev 'cd /home/box/work/muvon/synx && cargo fmt && cargo test && cargo clippy --all-targets'
   ```

3. **Wire format changes require a `PROTOCOL_VERSION` bump.** postcard
   encodes structs as untagged sequences: `#[serde(default)]` does NOT
   make an added field backward compatible (a truncated buffer fails with
   `DeserializeUnexpectedEnd`). Old/new binaries mixing must fail the
   version check with a clear message, not a decode error.

4. **Conventional commits** (`feat:`, `fix:`, `refactor:`, …), branches off
   `master`, rebase before merge. Never commit directly to `master` in
   shared work.

## Patterns

- **Errors:** `anyhow` throughout. Client classifies failures via
  `is_fatal` (config-level → exit) vs transient (→ reconnect with backoff).
  New fatal conditions must be added there.
- **Fs safety:** peer-requested mutations go through `peer.rs` apply
  functions → `resolve_beneath` confinement → atomic tmp+rename.
- **Testing layers:** unit tests in `src/*_tests.rs`; CLI-level integration
  in `tests/cli_process.rs` (runs the real binary); end-to-end client↔agent
  sessions via the fake-ssh shim — a `ssh` script doing
  `exec bash -c "${@: -1}"` earlier in `PATH` makes the client spawn the
  agent locally over pipes. Recreate BOTH roots fresh per case: Both-mode
  sync overwrites `.git/config`, so reusing a root corrupts its identity.
- **Known issue:** `--once` hangs after the initial sync (client blocks in
  `child.wait()`, agent never exits). Pre-existing; live mode is unaffected.

## Release

`cargo build --release`, version in `Cargo.toml`, tag `vX.Y.Z`, update
`CHANGELOG.md`. Installers: `install.sh` and the homebrew tap.
