# synx — sync semantics & invariants

Hard invariants behind deletion safety, suppression, and the git gate.
Opened from AGENTS.md → References. Violating any bullet here can silently
delete user data on a live repo — read fully before changing sync planning,
applies, baseline writes, or suppression.

## Sync semantics

- `.git/` **is synced by design**; the gate only pauses it during active git operations.
- At handshake both sides report normalized git remotes; both roots identifiable repos sharing zero remotes → client refuses (`--allow-repo-mismatch` overrides).
- Deletions propagate only with baseline evidence: the surviving copy must be byte-identical to the last converged state. First run has no baseline → nothing is deleted (stale-path safety).
- **The baseline may only record what the peer confirmed applying.** A path written there while the peer doesn't hold it is read as a deletion by the next session, which then deletes the last copy — that is how a live repo lost a commit. Two confirmation points, and no others: init sync (the peer's `SyncDone` follows every apply, and anything it failed arrives as `ApplyFailed` first — `sync.rs` keeps those paths out of the seed) and the live barrier (`LiveBaseline::begin_barrier` snapshots at `Ping`, `commit_barrier` writes it when `Pong` returns). Session teardown persists nothing: unconfirmed claims are dropped, and the next session re-derives them from the manifest exchange.
- `ApplyFailed { path, reason }` — not a bare `Error` — is what an apply failure owes the sender, so it can `LiveBaseline::forget` the path and void any snapshot claiming it.
- A `Ping` arriving while `.git/` ops sit in the gate's defer queue is queued behind them: answering early would confirm work that hasn't been applied. Teardown drains that queue (`GitGate::close`) rather than dropping it.
- Type mismatch (file vs dir vs symlink) → conflict surfaced, skipped, never blind-applied.
- The remote manifest is filtered through the **local** ignore stack before planning (agent doesn't know our rules).
- Echo suppression is state-based (recorded mtime/hash vs current on-disk state), not a time window — user edits during apply still flow.
- The stale-create guard fires only for deletes made **here** (`mark_deleted` / `mark_observed_deleted`, TTL-bounded). A delete applied on the peer's instruction uses `mark_applied_delete`: one connection delivers that peer's messages in order, so its later content for the path is newer, never stale. Dropping it (checkout/rebase removing a file and restoring it) loses the file here while the sender records it converged, and the next session reads that baseline as deletion evidence and removes the sender's copy too.
- Stale-`.git/` recovery (`sync.rs`): local `.git/` wiped to match remote only when the baseline proves `.git/` was previously converged; otherwise kept and pushed.

## Errors

`anyhow` throughout. Client classifies via `is_fatal` (`sync.rs`) — fatal strings:
`"protocol mismatch"`, `"invalid local path"`, `"remote must be "`, `"refusing to sync"`.
Anything else → reconnect with exponential backoff (1 s → 30 s cap). **New fatal
conditions must be added there** or the client reconnect-loops forever.

## Fs safety chain

peer-requested mutation → `peer.rs::apply_*` → `resolve_beneath` (rejects lexical
traversal and symlink ancestors) → tmp file **beside the destination**
(`.synx-tmp-<pid>-<nanos>`, same dir so rename(2) stays atomic) → rename.
The watcher filters `is_internal_temp` so our own writes never echo back.

## Troubleshooting

| Symptom | Cause / fix |
|---------|-------------|
| `DeserializeUnexpectedEnd` on connect | wire format changed without a `PROTOCOL_VERSION` bump |
| client reconnect-loops on a config error | error string missing from `is_fatal` — add it |
| repo identity corrupted after test runs | roots reused across Both-mode cases — recreate fresh |
| watcher silent under unreadable dirs | by design: `watch_subtree_tolerant` skips them and warns once; fix the perms |
| same files re-sync every session | clock skew between hosts in both mode (mtime-wins) — NTP or explicit `--mode` |
| protocol mismatch error at handshake | old/new binaries mixing — upgrade synx on both sides |
