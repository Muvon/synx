# synx — AGENTS.md

Fast, real-time bidirectional file sync over SSH. One binary, two roles: the
client (`synx LOCAL REMOTE`) spawns itself on the remote over SSH
(`synx --agent PATH`) and they speak a framed binary protocol over the SSH
pipe — no daemons, no new auth. Stack: Rust, tokio, postcard + zstd on the
wire, blake3 hashing, fast_rsync deltas. The `Message` enum in
`src/protocol.rs` **is** the entire client↔agent conversation — read it top
to bottom before touching anything else.

## Commands

- Build: `cargo build` · Release: `cargo build --release`
- Run: `cargo run -- <LOCAL> <REMOTE>` — uses real ssh; synx must exist on the remote too (`--remote-synx CMD` overrides)
- Full gate: `cargo fmt && cargo test && cargo clippy --all-targets`
- One module's tests only: `cargo test peer::` · CLI layer only: `cargo test --test cli_process`
- Coverage: `make coverage` (requires cargo-llvm-cov + llvm-tools-preview; excludes `*_tests.rs`)

## Where to look

| Task | Start here |
|------|------------|
| Add/change a CLI flag | `src/cli.rs` (`Cli` + `ClientArgs`) → wire through `src/main.rs` → `cli_tests.rs`; README quick-start if user-facing |
| Change the wire protocol | `src/protocol.rs` (`Message`, framing — **bump `PROTOCOL_VERSION`**) → dispatch in `peer.rs::handle_incoming`, `sync.rs`, `agent.rs`; round-trip test in `protocol_tests.rs` |
| Sync planning, deletions, conflicts | `sync.rs::build_plan` (three-way diff vs baseline) + `sync_tests.rs` |
| Apply safety / fs mutations | `peer.rs::apply_*` → `paths.rs::resolve_beneath`; tmp+rename via `tmp_path` |
| Live loop, echo suppression, coalescing | `peer.rs` (`live_loop`, `Suppression`, `coalesce`, `forward_local_events`); missed-events safety net `reconcile_sweep` |
| Git gate | `peer.rs::git_busy` / `GitGate` (`MARKERS`, `STALE_AFTER`, `GIT_SETTLE`) |
| Ignore rules | `ignores.rs` (`IgnoreStack`); remote manifest filtered through the **local** stack in `sync.rs`; also `watcher.rs` |
| Walk / hashing performance | `walker.rs` + `cache.rs` |
| Baseline / deletion evidence | `baseline.rs` (`Baseline` = read side, `LiveBaseline` = write side); loaded in `sync.rs` (stale-`.git/` recovery too) |
| Watcher backends, debounce, rename pairing | `watcher.rs` (`spawn`, `watch_subtree_tolerant`, `IdCache::resolve_rename`) |
| SSH invocation, remote parsing | `transport.rs` |
| Cut a release | bump `version` in `Cargo.toml`, tag **bare semver, no `v`** → details in `.agents/architecture.md` |

## Conventions

- Unit tests live in `src/<module>_tests.rs`, wired from the module file with `#[cfg(test)] #[path = "<module>_tests.rs"] mod tests;`. Test-only *helpers* (e.g. `Baseline::from_entries`) may stay in the module behind `#[cfg(test)]`; `#[test]` functions never do.
- New end-to-end behavior gets a **session-level test** over in-memory pipes: `run_inner(root, once_args(&root), false, Cursor::new(input), writer, None)` (`sync_tests.rs`), `run_io(root, Cursor::new(input), writer)` (`agent_tests.rs`); `encode()` builds wire bytes. No fake ssh, no child processes. (Layer 3, `tests/cli_process.rs`, runs the real binary via `CARGO_BIN_EXE_synx` for arg validation / early exits.)
- Conventional commits (`feat:`, `fix:`, `refactor:`, …); branches off `master`, rebase before merge.
- `CHANGELOG.md` is auto-generated from the commit log.

## Done

- `cargo fmt && cargo test && cargo clippy --all-targets` exits 0 (what CI runs on push/PR to `main`/`master`/`develop`).
- Any `Message` / wire-struct change: `PROTOCOL_VERSION` bumped and the `protocol_tests.rs` round-trip updated.
- Sync-semantics-relevant changes covered by a session-level test (layer 2), not only unit tests.

## Gotchas

- Recreate BOTH roots fresh per test case (the `TestDir` nonce helpers do this): Both-mode sync overwrites `.git/config`, so reusing a root corrupts its repo identity.
- `--once` hangs after the initial sync in real-process mode (client blocks in `child.wait()`, agent never exits; pre-existing, live mode unaffected — session tests pass `child = None`).
- New fatal error conditions must be added to `is_fatal` in `sync.rs`, or the client reconnect-loops forever on config errors.
- fast_rsync uses MD4 internally — the blake3 verify after `apply_delta_to_file` is the only honest integrity check; never remove it.
- Clock skew between hosts in Both mode → the same files re-sync every session (mtime-wins); NTP or explicit `--mode`.
- postcard encodes structs as untagged sequences — `#[serde(default)]` does NOT make an added field backward compatible (truncated buffer fails with `DeserializeUnexpectedEnd`).

## Never

- `#[test]` inline in a module file — always `src/<module>_tests.rs`.
- Change `Message` or any wire struct without bumping `PROTOCOL_VERSION`.
- Mutate peer-requested paths outside `peer.rs::apply_*`; never bypass `paths.rs::resolve_beneath`.
- Record in the baseline anything the peer hasn't confirmed applying — read `.agents/sync-semantics.md` before touching baseline/deletion logic.
- Special-case `.git/` out of the sync — it's synced by design; the git gate owns pausing.
- Hand-edit `CHANGELOG.md`.
- Commit directly to `master` in shared work.

## References

- `.agents/architecture.md` — read when navigating the codebase or tuning timings: module map, reading order, `peer.rs` internals, every tuning constant, CI and release details.
- `.agents/sync-semantics.md` — read before changing sync planning, applies, baseline, suppression, or the git gate: deletion/baseline invariants, error classification, fs safety chain, troubleshooting table.
