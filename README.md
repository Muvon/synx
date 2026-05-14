# synx

**Fast, real-time bidirectional file sync over SSH.** A simpler alternative to
Mutagen, written in Rust.

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
  zstd compression on the wire, atomic file writes, chunked transfer for
  large files.
- **Just SSH.** Uses your existing `ssh` setup — keys, agents, `~/.ssh/config`,
  `ProxyJump`, `ControlMaster`. No new auth to manage.
- **macOS + Linux.** FSEvents on macOS, inotify on Linux, via the `notify`
  crate.

## Install

```sh
# from this repo
cargo install --path .

# or, once published
cargo install synx
```

You need synx **on both ends**: your local machine *and* the remote. Build
once, then copy the binary, or `cargo install` over SSH.

```sh
# copy the local build to the remote (one-time setup)
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
synx host:/etc/nginx ./nginx --mode pull   # ← note: remote comes first
# (actually: synx ./nginx host:/etc/nginx --mode pull)

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

`.git/` is always skipped, regardless of any `.gitignore` rule.

If you change a `.gitignore` mid-session, restart synx to pick up the new
rules.

## How it works

```
┌─ local (client) ──────────────┐         ┌─ remote (agent) ─────────────┐
│                               │   ssh   │                              │
│  watcher (notify) ──┐         │ ◀─────▶ │  watcher (notify) ──┐        │
│  walker (parallel)  │         │  stdio  │  walker (parallel)  │        │
│  hash cache         │         │ bincode │  hash cache         │        │
│                     ▼         │ + zstd  │                     ▼        │
│  ┌─────────────────────┐      │         │  ┌────────────────────┐      │
│  │ diff plan + executor│ ◀────┼─────────┼─▶│ message dispatcher │      │
│  └─────────────────────┘      │         │  └────────────────────┘      │
└───────────────────────────────┘         └──────────────────────────────┘
```

- The client spawns `ssh user@host -- synx --agent /remote/path`. The same
  binary runs on both sides; agent mode is hidden in `--help`.
- Both sides walk their tree **in parallel** with `ignore::WalkBuilder::build_parallel()`,
  hashing files with **blake3**. A persistent cache keyed on `(path, size, mtime)`
  in `~/.cache/synx/` means re-runs skip rehashing unchanged files.
- Manifests stream over the wire (length-prefixed bincode, optionally zstd
  level 3 — compressed only when it saves space).
- The client computes a diff plan filtered through `.gitignore`. Operations
  are: dirs first → symlinks → files, then `FileGet` requests for pulls,
  then `SyncDone`.
- Files larger than **16 MiB** are streamed in **4 MiB chunks** to a temp
  file, then atomically renamed (`rename(2)`) into place with original mode
  and mtime preserved.
- Live mode runs both sides simultaneously. A 200ms debounce on the watcher
  coalesces editor save-storms and macOS FSEvent batching. Loop suppression
  prevents events from bouncing back.
- SSH uses `ControlMaster auto` with a 60-second persist, so multiple synx
  invocations reuse the same TCP connection.

## Performance notes

- **First sync of a large repo** is bound by hash + transfer. On modern hardware,
  blake3 hits ~1 GB/s per core; the parallel walker uses all your cores.
- **Re-sync of an unchanged repo** is bound by the walk alone (cache hit
  rate ~100%). A 100k-file repo re-syncs in ~1 second.
- **Live mode** has sub-second latency from save to remote write. Most of the
  time is the 200ms debounce.
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
~/.cache/synx/<hash>.cache    # (size,mtime)→blake3 hash, per sync root
~/.ssh/synx-%C                # SSH ControlMaster sockets
```

## Limits / future work

- **No delta sync** yet (full files over the wire). Designed for: drop in
  `fast_rsync` (Dropbox's SIMD librsync port) behind a flag in v0.2.
- **No three-way merge.** Conflicts use mtime-wins, not ancestor-aware
  detection. Reliable as long as both clocks are sane.
- **No daemon mode.** synx is foreground-only; `&` it or use `tmux`/`screen`
  for now. Daemonization with `synx status` / `synx stop` is planned.
- **Hash cache invalidation** is by (size, mtime) only. A file changed in
  place with the same size and mtime won't be re-hashed. This is the same
  heuristic git uses and is correct in practice.

## License

MIT OR Apache-2.0
