# AGENTS.md — synx development guide

Working rules and architecture for anyone (human or AI) contributing to synx.

## What this is

Fast, real-time bidirectional file sync over SSH. One binary, two roles:
the client (`synx LOCAL REMOTE`) spawns itself on the remote over SSH
(`synx --agent PATH`) and they speak a framed binary protocol over the
SSH pipe. No daemons, no new auth — the user's existing `ssh` setup is
the transport. Stack: Rust, tokio, postcard + zstd on the wire, blake3
hashing, fast_rsync deltas.

## Start here — reading order for a new session

1. `src/main.rs` (52 lines) — routes `--agent` → `agent::run`, else → `sync::run`.
2. `src/cli.rs` — every flag; `ClientArgs` is the client-side config struct.
3. `src/protocol.rs` — the `Message` enum **is** the entire client↔agent
   conversation. Read it top to bottom before touching anything else.
4. `src/sync.rs` — client lifecycle: `run` (reconnect loop, `is_fatal`)
   → `run_session` (spawn ssh, handshake, repo-mismatch check)
   → `run_inner` (walk, manifest exchange, `build_plan`, initial sync)
   → `peer::live_loop`.
5. `src/agent.rs` — `run_io`: the agent's mirror of steps 3–4.
6. `src/peer.rs` — everything both sides share (~2000 lines; internal map below).

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

## Project structure

```
synx/
├── src/
│   ├── main.rs             entrypoint; --agent routing
│   ├── cli.rs              clap defs; ClientArgs
│   ├── transport.rs        [user@]host:/path parsing; ssh command build/spawn
│   ├── protocol.rs         Message enum, framing, PROTOCOL_VERSION, size/chunk consts
│   ├── sync.rs             client: handshake, manifests, build_plan, initial sync, reconnect
│   ├── agent.rs            remote side: handshake, walk, applies client ops, forwards events
│   ├── peer.rs             shared: apply_*, live_loop, Suppression, GitGate, delta, Pending
│   ├── walker.rs           parallel manifest walk (blake3, all cores)
│   ├── cache.rs            persistent (size,mtime)→hash cache
│   ├── baseline.rs         persisted converged manifest (three-way diff ancestor)
│   ├── ignores.rs          per-directory .gitignore / .synxignore stack
│   ├── watcher.rs          notify (FSEvents/inotify), 200ms debounce, tolerant subtree watch, IdCache rename pairing
│   ├── paths.rs            resolve_beneath confinement; .synx-tmp- prefix
│   ├── ui.rs               terminal output
│   └── <module>_tests.rs   unit tests, one file per module (see Hard rules)
├── tests/cli_process.rs    CLI-level integration (runs the real binary)
├── install.sh              one-liner installer
├── CHANGELOG.md            auto-generated — never hand-edit
├── Makefile               make coverage (cargo-llvm-cov, excludes *_tests.rs)
└── .github/workflows/      ci.yml, release.yml (tag-driven), dependencies.yml (weekly dep-bump PRs)
```

## Where to look

| Task | Start here |
|------|------------|
| Add/change a CLI flag | `src/cli.rs` (`Cli` + `ClientArgs`) → wire through `main.rs` → `cli_tests.rs`; README quick-start if user-facing |
| Change the wire protocol | `src/protocol.rs` (`Message`, framing, `PROTOCOL_VERSION` — **bump it**) → dispatch in `peer.rs::handle_incoming`, `sync.rs`, `agent.rs`; round-trip test in `protocol_tests.rs` |
| Sync planning, deletions, conflicts | `sync.rs::build_plan` (three-way diff vs baseline) + `sync_tests.rs` |
| Apply safety / fs mutations | `peer.rs::apply_*` → `paths.rs::resolve_beneath`; tmp+rename via `tmp_path` |
| Live loop, echo suppression, coalescing | `peer.rs` (`live_loop`, `Suppression`, `coalesce`, `forward_local_events`) |
| Missed-events safety net | `peer.rs::reconcile_sweep` (30s stat-only sweep) |
| Git gate | `peer.rs::git_busy` / `GitGate` (MARKERS, STALE_AFTER, GIT_SETTLE) |
| Ignore rules | `ignores.rs` (`IgnoreStack`); applied in `sync.rs` (remote manifest filter) + `watcher.rs` |
| Walk / hashing performance | `walker.rs` + `cache.rs` |
| Baseline / deletion evidence | `baseline.rs`; loaded in `sync.rs` (also stale-.git/ recovery there) |
| SSH invocation, remote parsing | `transport.rs` |
| Watcher backends, debounce, unreadable dirs, rename pairing | `watcher.rs` (`spawn`, `watch_subtree_tolerant`, `IdCache::resolve_rename`) |
| Terminal output | `ui.rs` |
| CI behavior | `.github/workflows/ci.yml` — reusable `muvon/ci-workflow` rust-ci (stable, ubuntu+macos, beta on ubuntu) + musl build matrix (x86_64, aarch64) |
| Cut a release | `Cargo.toml` version + bare-semver tag (no `v`) → `release.yml` builds/publishes; `CHANGELOG.md` is auto-generated — don't edit it |
| Test coverage | `make coverage` (cargo-llvm-cov, `--ignore-filename-regex '_tests\.rs$'`) |
| Weekly dependency-bump PRs | `.github/workflows/dependencies.yml` — automated `chore: update dependencies` PR (cargo update + upgrade + audit); verify CI then merge |

## How things work

### peer.rs internal map (the big shared file)

- `apply_file_data` / `apply_mkdir` / `apply_symlink` / `apply_delete` / `apply_rename` — peer-requested mutations; all confined via `resolve_beneath`.
- `git_busy` / `GitGate` — detects active git ops (rebase/merge/cherry-pick/revert/bisect, `index.lock`, `HEAD.lock`), queues `.git/` traffic until quiet.
- `compute_signature` / `compute_delta` / `apply_delta_to_file` — fast_rsync deltas; result blake3-verified (fast_rsync uses MD4 internally, so blake3 is the only honest integrity check).
- `Pending` — chunked transfer state machine (`start`/`chunk`/`end`).
- `send_file` — push path: delta vs chunked vs whole-file; `is_precompressed` bypasses zstd for media/archives.
- `Suppression` — state-based echo suppression (`mark_set`/`mark_deleted`/`is_echo`).
- `SessionCtx` / `live_loop` / `handle_incoming` / `forward_local_events` / `coalesce` — the bidirectional live loop.
- `git_remotes` / `normalize_git_url` / `git_remotes_conflict` — wrong-repo protection.

### Key constants (the tuning knobs)

| Constant | Value | Where | Meaning |
|----------|-------|-------|---------|
| `PROTOCOL_VERSION` | 2 | protocol.rs | bump on ANY wire change |
| `MAX_MESSAGE_SIZE` | 64 MiB | protocol.rs | per-message cap |
| `COMPRESS_THRESHOLD` / `COMPRESS_LEVEL` | 512 B / 3 | protocol.rs | zstd above threshold |
| `IO_BUF_SIZE` | 64 KiB | protocol.rs | ssh stdio buffering |
| `CHUNK_THRESHOLD` / `CHUNK_SIZE` | 4 MiB / 4 MiB | protocol.rs | whole-file below one chunk, streamed above |
| `MAX_CONCURRENT_PUSHES` | 4 | sync.rs | semaphore-bounded pushes |
| `DELTA_MIN_SIZE` / `DELTA_MAX_SIZE` | 256 KiB / 256 MiB | sync.rs | delta-sync band; outside → full transfer |
| `RSYNC_BLOCK_SIZE` / `RSYNC_STRONG_LEN` | 4096 / 8 | peer.rs | fast_rsync signature params |
| `SUPPRESS_TTL` / `SUPPRESS_SWEEP` | 60 s / 5 s | peer.rs | echo-suppression entry lifetime |
| `RECONCILE_INTERVAL` | 30 s | peer.rs | missed-events sweep; skipped when the watcher was silent |
| `STALE_AFTER` | 600 s | peer.rs | git markers older → ignored (crashed git self-heals) |
| `GIT_SETTLE` | 5 s | peer.rs | quiet period after git finishes |
| `DEBOUNCE` / `DEBOUNCE_TICK` | 200 ms / 100 ms | watcher.rs | editor save-storm coalescing / flush wakeup |

### Sync semantics

- `.git/` **is synced by design**; the gate only pauses it during active git operations.
- At handshake both sides report normalized git remotes; both roots identifiable repos sharing zero remotes → client refuses (`--allow-repo-mismatch` overrides).
- Deletions propagate only with baseline evidence: the surviving copy must be byte-identical to the last converged state. First run has no baseline → nothing is deleted (stale-path safety).
- Type mismatch (file vs dir vs symlink) → conflict surfaced, skipped, never blind-applied.
- The remote manifest is filtered through the **local** ignore stack before planning (agent doesn't know our rules).
- Echo suppression is state-based (recorded mtime/hash vs current on-disk state), not a time window — user edits during apply still flow.
- Stale-`.git/` recovery (`sync.rs`): local `.git/` wiped to match remote only when the baseline proves `.git/` was previously converged; otherwise kept and pushed.

### Errors

`anyhow` throughout. Client classifies via `is_fatal` (`sync.rs`) — fatal strings:
`"protocol mismatch"`, `"invalid local path"`, `"remote must be "`, `"refusing to sync"`.
Anything else → reconnect with exponential backoff (1s → 30s cap). **New fatal
conditions must be added there** or the client reconnect-loops forever.

### Fs safety chain

peer-requested mutation → `peer.rs::apply_*` → `resolve_beneath` (rejects lexical
traversal and symlink ancestors) → tmp file **beside the destination**
(`.synx-tmp-<pid>-<nanos>`, same dir so rename(2) stays atomic) → rename.
The watcher filters `is_internal_temp` so our own writes never echo back.

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

2. **Wire format changes require a `PROTOCOL_VERSION` bump.** postcard
   encodes structs as untagged sequences: `#[serde(default)]` does NOT
   make an added field backward compatible (a truncated buffer fails with
   `DeserializeUnexpectedEnd`). Old/new binaries mixing must fail the
   version check with a clear message, not a decode error.

3. **Conventional commits** (`feat:`, `fix:`, `refactor:`, …), branches off
   `master`, rebase before merge. Never commit directly to `master` in
   shared work.

## Testing — three layers

1. **Unit** — `src/<module>_tests.rs` (wiring above).
2. **Session-level, no ssh** — drive the session functions directly with
   in-memory pipes: `sync_tests.rs` calls
   `run_inner(root, once_args(&root), false, Cursor::new(input), writer, None)`;
   `agent_tests.rs` calls `run_io(root, Cursor::new(input), writer)`. The
   `encode()` helper builds wire bytes; decode replies by looping
   `read_message` over the shared `Vec<u8>` writer. **This is the pattern
   for new end-to-end behavior** — no fake ssh, no child processes.
3. **CLI process** — `tests/cli_process.rs` runs the real binary via
   `CARGO_BIN_EXE_synx` (arg validation, early exits).

Gotchas:
- Recreate BOTH roots fresh per case (the `TestDir` nonce helper does this):
  Both-mode sync overwrites `.git/config`, so reusing a root corrupts its identity.
- `--once` hangs after the initial sync in real-process mode (client blocks
  in `child.wait()`, agent never exits). Pre-existing; live mode unaffected;
  session tests pass `child = None` and don't hit it.

## Validation

```bash
# full gate
cargo fmt && cargo test && cargo clippy --all-targets

# one module's tests only
cargo test peer::

# CLI layer only
cargo test --test cli_process

# coverage report (requires cargo-llvm-cov + llvm-tools-preview)
make coverage
```

CI runs on push/PR to `main`/`master`/`develop`: fmt/test/clippy via the
reusable `muvon/ci-workflow` rust-ci job (stable on ubuntu+macos, beta on
ubuntu), a coverage job (badge JSON pushed to the `badges` branch), musl
builds for x86_64 and aarch64, and a `brief` job. Separately,
`dependencies.yml` opens a weekly `chore: update dependencies` PR.

## Troubleshooting

| Symptom | Cause / fix |
|---------|-------------|
| `DeserializeUnexpectedEnd` on connect | wire format changed without a `PROTOCOL_VERSION` bump |
| client reconnect-loops on a config error | error string missing from `is_fatal` — add it |
| repo identity corrupted after test runs | roots reused across Both-mode cases — recreate fresh |
| watcher silent under unreadable dirs | by design: `watch_subtree_tolerant` skips them and warns once; fix the perms |
| same files re-sync every session | clock skew between hosts in both mode (mtime-wins) — NTP or explicit `--mode` |
| protocol mismatch error at handshake | old/new binaries mixing — upgrade synx on both sides |

## Never

- `#[test]` inline in a module file — always `src/<module>_tests.rs`.
- Change `Message` or wire structs without bumping `PROTOCOL_VERSION`.
- Mutate peer-requested paths outside `peer.rs::apply_*` (never bypass `resolve_beneath`).
- Commit directly to `master` in shared work.
- Special-case `.git/` out of the sync — it's synced by design; the git gate owns pausing.
- Hand-edit `CHANGELOG.md` — it's auto-generated from the commit log.

## Release

1. Bump `version` in `Cargo.toml`.
2. Don't touch `CHANGELOG.md` — it's auto-generated from the commit log.
3. Tag `0.1.4` — **bare semver, no `v` prefix** (release.yml's tag filter is
   `[0-9]+.[0-9]+.[0-9]+*`) — and push it. `release.yml` then builds all four
   targets (musl ×2, darwin ×2), publishes to crates.io, and creates the
   GitHub release; `workflow_dispatch` can release a given tag manually.
   Installers: `install.sh` (pulls the GitHub-release tarballs) and the
   homebrew tap.
