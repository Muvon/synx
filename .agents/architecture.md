# synx — architecture reference

Depth for agents navigating the codebase or tuning behavior. Opened from
AGENTS.md → References; nothing here overrides it.

## The two roles

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

One binary, two roles: `synx LOCAL REMOTE` (client) spawns `synx --agent PATH`
on the remote over SSH; the user's existing `ssh` setup is the transport —
no daemons, no new auth.

## Reading order for a new session

1. `src/main.rs` — routes `--agent` → `agent::run`, else builds `ClientArgs` → `sync::run`.
2. `src/cli.rs` — every flag; `ClientArgs` is the client-side config struct.
3. `src/protocol.rs` — the `Message` enum **is** the entire client↔agent
   conversation. Read it top to bottom before touching anything else.
4. `src/sync.rs` — client lifecycle: `run` (reconnect loop, `is_fatal`)
   → `run_session` (spawn ssh, handshake, repo-mismatch check)
   → `run_inner` (walk, manifest exchange, `build_plan`, initial sync)
   → `peer::live_loop`.
5. `src/agent.rs` — `run_io`: the agent's mirror of steps 3–4.
6. `src/peer.rs` — everything both sides share (~2200 lines; map below).

## Module map

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
│   ├── watcher.rs          notify (FSEvents/inotify), debounce, tolerant subtree watch, IdCache rename pairing
│   ├── paths.rs            resolve_beneath confinement; .synx-tmp- prefix
│   ├── ui.rs               terminal output
│   └── <module>_tests.rs   unit tests, one file per module (see AGENTS.md)
├── tests/cli_process.rs    CLI-level integration (runs the real binary)
├── install.sh              installer (pulls GitHub-release tarballs)
├── CHANGELOG.md            auto-generated — never hand-edit
├── Makefile                make coverage (cargo-llvm-cov, excludes *_tests.rs)
└── .github/workflows/      ci.yml, release.yml (tag-driven), dependencies.yml (weekly dep-bump PR)
```

## peer.rs internal map (the big shared file)

- `apply_file_data` / `apply_mkdir` / `apply_symlink` / `apply_delete` / `apply_rename` — peer-requested mutations; all confined via `resolve_beneath`.
- `git_busy` / `GitGate` — detects active git ops (rebase/merge/cherry-pick/revert/bisect markers, `index.lock`, `HEAD.lock`), queues `.git/` traffic until quiet.
- `compute_signature` / `compute_delta` / `apply_delta_to_file` — fast_rsync deltas; result blake3-verified (fast_rsync uses MD4 internally, so blake3 is the only honest integrity check).
- `Pending` — chunked transfer state machine (`start`/`chunk`/`end`).
- `send_file` — push path: delta vs chunked vs whole-file; `is_precompressed` bypasses zstd for media/archives.
- `Suppression` — state-based echo suppression (`mark_set`/`mark_deleted`/`mark_applied_delete`/`mark_observed_deleted`/`is_echo`).
- `SessionCtx` / `live_loop` / `handle_incoming` / `forward_local_events` / `coalesce` — the bidirectional live loop.
- `git_remotes` / `normalize_git_url` / `git_remotes_conflict` — wrong-repo protection.

## Tuning constants (the knobs)

| Constant | Value | Where | Meaning |
|----------|-------|-------|---------|
| `PROTOCOL_VERSION` | 4 | protocol.rs | bump on ANY wire change |
| `MAX_MESSAGE_SIZE` | 64 MiB | protocol.rs | per-message cap |
| `COMPRESS_THRESHOLD` / `COMPRESS_LEVEL` | 512 B / 3 | protocol.rs | zstd above threshold |
| `IO_BUF_SIZE` | 64 KiB | protocol.rs | ssh stdio buffering |
| `CHUNK_THRESHOLD` / `CHUNK_SIZE` | 4 MiB / 4 MiB | protocol.rs | whole-file below one chunk, streamed above |
| `MAX_CONCURRENT_PUSHES` | 4 | sync.rs | semaphore-bounded pushes |
| `DELTA_MIN_SIZE` / `DELTA_MAX_SIZE` | 256 KiB / 256 MiB | sync.rs | delta-sync band; outside → full transfer |
| `RSYNC_BLOCK_SIZE` / `RSYNC_STRONG_LEN` | 4096 / 8 | peer.rs | fast_rsync signature params |
| `SUPPRESS_TTL` / `SUPPRESS_SWEEP` | 60 s / 5 s | peer.rs | echo-suppression entry lifetime |
| `RECONCILE_INTERVAL` | 30 s | peer.rs | missed-events sweep; skipped when the watcher was silent |
| `BARRIER_INTERVAL` | 3 s | peer.rs | `Ping`/`Pong` that confirms the baseline; silent when nothing is pending |
| `STALE_AFTER` | 600 s | peer.rs (`git_busy`) | git markers older → ignored (crashed git self-heals) |
| `GIT_SETTLE` | 5 s | peer.rs | quiet period after git finishes |
| `DEBOUNCE` / `DEBOUNCE_TICK` | 200 ms / 100 ms | watcher.rs | editor save-storm coalescing / flush wakeup |
| `MOVED_CAP` | 10 000 | watcher.rs | bound on remembered removed ids for rename pairing |
| `MMAP_HASH_THRESHOLD` | 1 MiB | walker.rs | below it, mmap setup cost outweighs parallelism |

## CI (`.github/workflows/ci.yml`)

Runs on push/PR to `main`/`master`/`develop`; newer pushes cancel in-progress
runs for the same ref. Jobs: `rust` via reusable `muvon/ci-workflow` rust-ci
(stable on ubuntu+macos, beta on ubuntu), `coverage` (badge JSON pushed to the
`badges` branch), `musl-build` for x86_64 and aarch64, and `brief`.

`dependencies.yml` runs weekly (Mon 10:00 UTC): `cargo update` +
`cargo upgrade --incompatible` + `cargo test` + `cargo audit`, then opens a
`chore: update dependencies` PR (branch `chore/update-dependencies`).
Verify CI then merge.

## Release

1. Bump `version` in `Cargo.toml`.
2. Don't touch `CHANGELOG.md` — it's auto-generated from the commit log.
3. Tag **bare semver, no `v` prefix** (release.yml's tag filter is
   `[0-9]+.[0-9]+.[0-9]+*`) and push it. `release.yml` then builds all four
   targets (musl ×2, darwin ×2), publishes to crates.io, and creates the
   GitHub release. `workflow_dispatch` can release a given tag manually.
   Installer: `install.sh` (pulls the GitHub-release tarballs).
