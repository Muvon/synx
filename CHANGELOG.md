# Changelog

## [0.1.2] - 2026-08-07

### 📋 Release Summary

This release improves data safety during synchronization by verifying Git repository presence before performing wipe operations (7d69862d).


### ✨ New Features & Enhancements

- **sync**: verify git presence via baseline before wiping `7d69862d`

## [0.1.1] - 2026-07-26

### 📋 Release Summary

This release significantly improves system performance through optimized I/O, memory usage, and faster file synchronization (a7d64dab, 971feb44). Users will also benefit from enhanced filesystem security, increased stability, and expanded technical documentation (945b9bf3, 8dc1e45f).


### 🔧 Improvements & Optimizations

- **core**: enhance filesystem security and stability `945b9bf3`
- **manifest**: optimize walking and caching efficiency `a7d64dab`
- **peer**: format line wraps `06a311d5`
- **agent**: optimize I/O, manifest sync and memory `971feb44`

### 📚 Documentation & Examples

- **readme**: expand feature set and technical details `8dc1e45f`

All notable changes to this project will be documented in this file.

## [Unreleased]

### Performance

- Fold nested ignore-file discovery into the parallel manifest walk, removing
  one complete tree traversal at startup while arming the watcher before any
  scan begins.
- Make manifest cache reads immutable and batch walker results per worker,
  eliminating the global per-entry mutex and channel handoff.
- Reuse walker metadata, skip unchanged cache/baseline writes, and bypass zstd
  for common pre-compressed file formats.

## [0.1.0] - 2026-07-06

### 📋 Release Summary

This release introduces fast, real-time bidirectional file synchronization featuring an efficient delta transfer protocol, parallelized processing, and intelligent Git integration (3091d4bf, b347aae1, c2ea0c1e, 66355e36, f2575a89). Connection resilience and data integrity have been significantly improved through enhanced file handling, content-based deduplication, and atomic operations (d08a2d2e, 8c9eb535, c3fd1eb6). Additionally, several bug fixes resolve issues with file conflicts, deletion persistence, and watcher reliability to ensure a more stable syncing experience (b9f12868, 182ca3fe, c412643d, c7dc3121).


### ✨ New Features & Enhancements

- **sync**: implement live baseline and git gate `66355e36`
- **sync**: implement three-way diff for deletions `6d0ba9b4`
- **sync**: pause .git synchronization during active git operations `f2575a89`
- **agent**: implement event suppression and race prevention `09b2efd6`
- **agent**: improve file handling and connection resilience `d08a2d2e`
- **sync**: implement rsync-style delta transfer protocol `b347aae1`
- **sync**: parallelize file hashing and transfers `c2ea0c1e`
- **peer**: implement content-based deduplication via Touch `8c9eb535`
- **sync**: implement mtime-based echo suppression `311ec2b1`
- **core**: initialize synx real-time file synchronization utility `3091d4bf`

### 🔧 Improvements & Optimizations

- **github**: migrate to shared and reusable workflows `154dd330`
- **sync**: consolidate session state into SessionCtx `eb486ac5`
- **protocol**: replace bincode with postcard serialization `a6b975a1`
- **legal**: migrate project license to Apache-2.0 `754a933b`
- **peer**: enforce atomic renames for file finalization `c3fd1eb6`
- **github**: implement comprehensive ci/cd and installer `a0855800`

### 🐛 Bug Fixes & Stability

- **sync**: recover from stale local git state `c7dc3121`
- **peer**: ignore stale git lock markers `e014209c`
- **sync**: handle file operation errors and type conflicts gracefully `b9f12868`
- **peer**: verify blake3 hash of chunked transfers `b85524aa`
- **sync**: prevent stale file resurrection after deletion `182ca3fe`
- **sync**: improve file matching and watcher reliability `c412643d`
- **peer**: suppress duplicate logs on remote agent `d0db3806`

### 🔄 Other Changes

6 maintenance, dependency, and tooling updates not listed individually.
