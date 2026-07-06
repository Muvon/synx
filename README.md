# synx

> Fast, real-time bidirectional file sync over SSH.

A simpler alternative to Mutagen, written in Rust.

[![CI](https://github.com/Muvon/synx/actions/workflows/ci.yml/badge.svg)](https://github.com/Muvon/synx/actions/workflows/ci.yml)
[![Release](https://github.com/Muvon/synx/actions/workflows/release.yml/badge.svg)](https://github.com/Muvon/synx/actions/workflows/release.yml)
[![License: Apache-2.0](https://img.shields.io/badge/license-Apache--2.0-blue.svg)](LICENSE)
[![crates.io](https://img.shields.io/crates/v/synx.svg)](https://crates.io/crates/synx)

```
synx  /Users/dk/proj  ◀─▶  dev@beefy:/srv/proj
✓ connected
• manifests:  local 1243  •  remote 1180 (47 ignored)
• plan: push 78 files (4.2 MiB) 6 dirs 0 links  •  pull 14 entries
✓ initial sync: 4.2 MiB sent, 312 KiB received in 1.4s
• watching for changes — ctrl+c to stop
  → src/main.rs  3.1 KiB
  ← README.md   824 B
```

## Why synx

- **One command, no daemons.** `synx ./here you@there:/path` — that's it. No
  config files, no session manager, no agents to register.
- **Respects `.gitignore` everywhere.** Nested `.gitignore` files at any depth
  are loaded and applied authoritatively — files that match are *never* synced,
  in either direction.
- **Real-time, both ways.** A debounced filesystem watcher on each side ships
  changes the moment they happen. Push, pull, or two-way (default).
- **Production-grade transfer.** Parallel hashing (blake3 + multi-threaded
  walk), persistent hash cache (re-runs skip re-hashing unchanged files),
  rsync-style delta sync for large mutable files, zstd compression on the wire,
  atomic file writes, chunked transfer for large files.
- **Just SSH.** Uses your existing `ssh` setup — keys, agents, `~/.ssh/config`,
  `ProxyJump`, `ControlMaster`. No new auth to manage.
- **Git-aware.** Pauses `.git/` synchronization when it detects an active git
  operation (rebase, merge, cherry-pick, etc.) so you never corrupt a repo
  mid-operation.
- **macOS + Linux.** FSEvents on macOS, inotify on Linux, via the `notify`
  crate.

## Install

```sh
# one-liner (Linux & macOS, x86_64 + ARM64)
curl -fsSL https://raw.githubusercontent.com/Muvon/synx/main/install.sh | sh

# from crates.io
cargo install synx

# from this repo
cargo install --path .
```

You need synx **on both ends**: your local machine *and* the remote. The
quickest way is to run the one-liner on each host. If you already have a
local release build:

```sh
scp target/release/synx user@host:~/.local/bin/synx
ssh user@host 'chmod +x ~/.local/bin/synx'
```

## Quick start

```sh
# two-way sync (default)
synx ./src dev@host:/srv/app/src

# one-way push (local → remote)
synx ./build host:/var/www --mode push

# one-way pull (remote → local)
synx ./nginx host:/etc/nginx --mode pull

# do the initial sync and exit (no live watch)
synx ./code host:/work --once

# show what would happen, do nothing
synx ./code host:/work --dry-run

# verbose logging
synx ./code host:/work -v        # debug
synx ./code host:/work -vv       # trace

# SSH on a non-standard port
synx ./code host:/work --ssh-opts "-p 2222"

# synx isn't in PATH on the remote
synx ./code host:/work --remote-synx ~/.local/bin/synx

# tilde expansion works
synx ~/notes host:~/notes
```

## Sync modes

| Mode | Direction | Conflict | Initial sync |
|------|-----------|----------|--------------|
| `push` | local → remote | local always wins | only sends local-only or differing files |
| `pull` | remote → local | remote always wins | only fetches remote-only or differing files |
| `both` (default) | bidirectional | **newer mtime wins** | merges (no deletions on first sync) |

In `both` mode, **deletions only propagate during live watch** — the initial
sync never deletes files, so accidentally running `synx` against a stale path
won't blow away your data. If a file vanishes while synx is watching, the
deletion is propagated.

## Ignore rules — authoritative

synx loads **every `.gitignore`** under your sync root (plus an optional
`.synxignore` with identical syntax) and treats them as the single source of
truth for what *isn't* synced. This applies to:

1. **The initial walk** — ignored files never enter the manifest.
2. **The remote manifest** — files reported by the remote that match your
   local `.gitignore` are filtered out *before* the diff plan, so you don't
   accidentally pull a `target/` or `node_modules/` that the remote happened
   to have.
3. **Live events** — both incoming applies and outgoing notifications skip
   ignored paths.

**Dotfiles are NOT special.** `.env`, `.git/`, `.vscode/`, etc. are synced
just like any other path unless your `.gitignore` (or `.synxignore`) excludes
them. If you don't want to sync `.git/`, add a line to `.synxignore`:

```sh
echo '/.git' >> .synxignore
```

If you change a `.gitignore` mid-session, restart synx to pick up the new
rules.

## How it works

```
┌─ local (client) ──────────────┐         ┌─ remote (agent) ─────────────┐
│                               │   ssh   │                              │
│  watcher (notify) ──┐         │ ◀─────▶ │  watcher (notify) ──┐        │
│  walker (parallel)  │         │  stdio  │  walker (parallel)  │        │
│  hash cache         │         │ postcard│  hash cache         │        │
│                     ▼         │ + zstd  │                     ▼        │
│  ┌─────────────────────┐      │         │  ┌────────────────────┐      │
│  │ diff plan + executor│ ◀────┼─────────┼─▶│ message dispatcher │      │
│  └─────────────────────┘      │         │  └────────────────────┘      │
└───────────────────────────────┘         └──────────────────────────────┘
```

- The client spawns `ssh user@host -- synx --agent /remote/path`. The same
  binary runs on both sides; agent mode is hidden from `--help`.
- Both sides walk their tree **in parallel** with `ignore::WalkBuilder::build_parallel()`,
  hashing files with **blake3**. A persistent cache keyed on `(path, size, mtime)`
  in `~/.cache/synx/` means re-runs skip rehashing unchanged files.
- Manifests stream over the wire (length-prefixed postcard, optionally zstd
  level 3 — compressed only when it saves space).
- The client computes a diff plan filtered through `.gitignore`. Operations
  are: dirs first → symlinks → files, then `FileGet` requests for pulls,
  then `SyncDone`.
- **Delta sync.** Files between 256 KiB and 256 MiB where the remote already
  has a different version are synced via rsync-style deltas (`fast_rsync`,
  SIMD-accelerated librsync). The receiver requests a signature of its
  current copy, the sender computes a delta against it, and the result is
  verified with blake3 before accepting — fast_rsync uses MD4 internally,
  so the blake3 check is the only honest integrity guarantee. Files outside
  the delta band fall back to full transfer.
- Files larger than **16 MiB** are streamed in **4 MiB chunks** to a temp
  file, then atomically renamed (`rename(2)`) into place with original mode
  and mtime preserved.
- **Three-way deletion diff.** A persistent baseline manifest records what
  both sides agreed on at the last successful sync. Without it, "the user
  deleted this file here" and "the peer created this file there" are
  indistinguishable — a stateless diff would silently resurrect deletions.
  The baseline lets synx classify a missing path as a genuine deletion
  (propagate) or a concurrent change (keep, never lose data).
- **Git gate.** When synx detects an active git operation (rebase, merge,
  cherry-pick, revert, bisect, or a live `index.lock` / `HEAD.lock`), it
  pauses `.git/` synchronization and queues `.git/` events until the
  operation finishes. Stale markers older than 10 minutes are ignored, so
  a crashed git self-heals.
- Live mode runs both sides simultaneously. A 200ms debounce on the watcher
  coalesces editor save-storms and macOS FSEvent batching. Per-path event
  coalescing inside each batch ensures `Create+Modify` (typical editor save)
  becomes a single push, not two.
- **Echo suppression is state-based, not time-based.** When we apply an
  incoming change, we record the resulting mtime (or "deleted"). When our
  watcher subsequently fires for that path, we compare the **current** on-disk
  mtime to what we recorded — only a *matching* state is treated as an echo
  and dropped. If the user has modified the file in the meantime, the event
  flows through normally. This means there's no time window during which
  legitimate user edits are blocked.
- The receiver also does **content dedup**: if an incoming `FileData` matches
  what's already on disk (same size + mtime), the apply is skipped entirely.
  Wasted bandwidth on the wire, but no on-disk churn and no log spam.
- SSH uses `ControlMaster auto` with a 60-second persist, so multiple synx
  invocations reuse the same TCP connection.

## Performance notes

- **First sync of a large repo** is bound by hash + transfer. On modern hardware,
  blake3 hits ~1 GB/s per core; the parallel walker uses all your cores.
- **Re-sync of an unchanged repo** is bound by the walk alone (cache hit
  rate ~100%). A 100k-file repo re-syncs in ~1 second.
- **Live mode** has sub-second latency from save to remote write. Most of the
  time is the 200ms debounce.
- **Delta sync** cuts wire traffic on large mutable files (logs, dumps,
  binaries that change slightly) — only the changed blocks are sent.
- **Compression** is on by default (zstd level 3). For local-network sync of
  binary blobs that don't compress, `--no-compress` is faster.

## Troubleshooting

**"synx: command not found"** on the remote side.
The agent must be in `$PATH` of the remote login shell. Either install it
there (`cargo install synx`) or pass `--remote-synx /full/path/to/synx`.

**Protocol mismatch.**
`synx 0.1` is wire-incompatible with future versions. Upgrade both sides.

**"Permission denied" on initial sync.**
SSH credentials issue, not a synx bug. Try `ssh user@host` manually first.

**Files keep getting re-synced.**
Most often clock skew between local and remote in `both` mode — the side
with the future clock always "wins". Either set both clocks via NTP or use
`--mode push` / `--mode pull` explicitly.

**Large file fails to transfer.**
synx caps per-message size at 64 MiB; for files larger than the chunk
threshold (16 MiB) it uses streaming chunks of 4 MiB, so there's no
practical file-size limit. If you hit `message too large`, file a bug — it
shouldn't be reachable.

**`target/` (or `node_modules/`) is getting synced anyway.**
If the file already exists on the remote, synx 0.1 will NOT delete it
during initial sync (the safer two-way behavior). It also won't push or pull
it from now on (the `.gitignore` filter blocks that). To clean up old
ignored files on the remote, delete them by hand once.

## Configuration

There's no config file. Everything is CLI flags. Persistent state:

```
~/.cache/synx/<hash>.cache       # (size,mtime)→blake3 hash, per sync root
~/.cache/synx/<hash>.baseline    # last converged manifest, per sync root
~/.ssh/synx-%C                   # SSH ControlMaster sockets
```

## Limits / future work

- **No three-way content merge.** Conflicts use mtime-wins, not
  ancestor-aware content merging. Deletions are three-way (baseline-backed),
  but file content conflicts are not. Reliable as long as both clocks are
  sane.
- **No daemon mode.** synx is foreground-only; `&` it or use `tmux`/`screen`
  for now. Daemonization with `synx status` / `synx stop` is planned.
- **Hash cache invalidation** is by (size, mtime) only. A file changed in
  place with the same size and mtime won't be re-hashed. This is the same
  heuristic git uses and is correct in practice.

## Contributing

```sh
# clone and build
git clone https://github.com/Muvon/synx.git
cd synx
cargo build --release

# run tests
cargo test

# run synx locally against a remote
./target/release/synx ./src user@host:/srv/src -v
```

CI runs on every push (`.github/workflows/ci.yml`). Releases are built and
published automatically from tags (`.github/workflows/release.yml`).

## License

Apache-2.0 — see [LICENSE](LICENSE).