# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

EmveDB is an embedded, single-file vector database library crate (Rust 2024). It stores
fixed-dimension `f32` vectors plus opaque payload bytes in one append-only file, with exact
brute-force top-k search. No server process.

See `AGENTS.md` for contribution conventions (commit style, naming, testing expectations) and
`SPEC.md` for the authoritative on-disk format design (written in Japanese).

## Commands

```sh
cargo build
cargo test                                # full suite (unit + integration)
cargo test --test search_public_api       # one integration target (also: db_public_api)
cargo test memory_put_get                 # single test by name substring
cargo run --example basic_search          # also: filters, file_persistence
cargo fmt --check
cargo clippy --all-targets -- -D warnings
```

Unit tests live in `#[cfg(test)] mod tests` next to implementation; black-box tests are in
`tests/`. `criterion` is a dev-dependency but there is currently no `benches/` directory.

## Architecture

The crate is strictly layered. Each layer only knows about the one below it, which is the key
to navigating the code:

- **`EmveDb` (`db.rs`)** — public, thread-safe facade. Wraps `RwLock<InnerDb>` and caches a
  `DbMeta` (dimension, metric, open_mode, max_payload_size) so reads don't lock for metadata.
  Performs input validation (dimension match, finite values, payload size, read-only/k==0
  guards) before delegating. Acquires the write lock for `put`/`delete`/`flush`/`compact`, the
  read lock for `get`/`search`/`len`.
- **`InnerDb` (`inner_db.rs`)** — single-threaded core that orchestrates storage + index. The
  real workflow lives here. It re-validates inputs (defense in depth — the public layer
  already checked) and owns `header`, `index`, `sync_mode`, `open_mode`. `Drop` syncs on
  read-write handles.
- **`EmvedbStorage<S>` (`storage/storage.rs`)** — format-aware wrapper over a raw `Storage`.
  Understands the 64-byte header at offset 0 and the length-prefixed frame stream after it.
  `read_all_frames()` linearly scans the whole file.
- **`Storage` trait (`storage/mod.rs`)** — raw byte backend (`append`, `read_at`, `len`,
  `sync`, `write_header`, `recreate`). Used boxed as `Box<dyn Storage>`. Two impls:
  `FileStorage` (`storage/file.rs`) and `MemoryStorage` (`storage/memory.rs`).
- **`InMemoryIndex` (`index.rs`)** — the in-memory source of truth for all reads and search.
  `HashMap<u64, Entry>` maps id → slot + payload location + cached `inv_norm`; vectors are
  packed into one flat `arena: Vec<f32>` addressed by `slot * dimension`; `free_slots` recycles
  arena space on update/delete. The file is only a durable log — the index is what queries hit.

### Append-only log + replay model (central concept)

Writes never overwrite. `put` and `delete` append a `Frame` (Put or Delete tombstone) to the
end of the file. On `open`/`create`, `EmvedbStorage::read_all_frames()` scans every frame and
`InMemoryIndex::build_from_frames()` replays them in order to reconstruct live state (later
frames for the same id win; Delete removes). `compact()` rewrites a fresh file containing only
live records, bumps the header `generation`, and atomically swaps it in via `recreate`
(write `.compact` temp → rename original to `.compact.old` → rename temp into place). Stale
`.compact`/`.compact.old` artifacts are cleaned up on the next open.

### On-disk format (see SPEC.md for the full spec)

- **Header** (`format/header.rs`, 64 bytes): MAGIC `EMVEDB\0\0`, format_version (=1),
  generation, metric, element_type, dimension, flags, trailing crc32c. Decoding verifies
  magic, CRC, and version.
- **Frame** (`format/frame.rs`): `[u32 len][body][u32 crc32c]`, little-endian. `body[0]` is the
  type tag (`1`=Put, `2`=Delete), `body[1]` is flags. Put body = id (u64), payload_len (u32),
  vector (`f32 * dimension`), payload bytes. CRC covers the length bytes + body. `FrameRef`
  carries the decoded vector plus the payload's absolute byte range (payloads are read lazily
  from storage, not held in the index).

### Crash recovery & integrity

`FileStorage::validate()` runs on every open and distinguishes a torn/CRC-bad **tail** frame
(recoverable: truncate to last good offset; read-write opens actually `set_len`, read-only just
caps the logical length) from corruption in the **middle** of the file (`EmveError::Corrupt`,
non-recoverable). `logical_len` tracks the valid data length independently of the physical file
size, so `read_at` never reads past validated data.

### Search & metrics (`metric.rs`, `search.rs`, `heap.rs`)

`SearchResultItem::score` is always "larger is better"; `distance` is metric-specific. Cosine
caches `inv_norm` per vector (and computes it once for the query); a zero-norm vector yields
`(-inf, +inf)`. Top-k uses a bounded min-heap (`BinaryTopKMinHeap`, size k). `SearchResultItem`
ordering breaks score ties by **smaller id first**. Filtering goes through the `SearchFilter`
trait: `filter_id` runs before scoring, `filter` runs on the scored result; `with_filter`
(a `Fn(u64) -> bool`), `with_result_filter`, and `with_min_score` are the ergonomic builders.

### Conventions specific to this crate

- `:memory:` as the path selects `MemoryStorage`; it cannot be reopened with `open`
  (`EmveError::CannotOpenMemory`). `recreate`/compact on memory just swaps the in-RAM buffer.
- Dimension must be `1..=65535`; element type is `F32` only (`element_type.rs` reserves the
  byte for future types).
- Wrap fallible expressions whose errors you want traced in the `with_debug_log!` macro
  (`error.rs`) — it attaches a `tracing::debug!` on the `Err` path. `tracing` is used
  throughout; tests use `tracing-test`.
- Public errors are the `EmveError` enum with a crate `Result<T>` alias; prefer specific
  variants (`DimensionMismatch`, `PayloadTooLarge`, `ReadOnly`, `Locked`, …) over `Corrupt`.

### Concurrency & locking

In-process concurrency is the `RwLock<InnerDb>` in `EmveDb`. Cross-process coordination uses
advisory file locks (`storage/lock.rs`): exclusive for read-write, shared for read-only;
`lock_wait` chooses blocking vs. immediate `EmveError::Locked`.
