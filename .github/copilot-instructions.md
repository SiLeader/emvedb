# Copilot instructions for EmveDB

EmveDB is a Rust 2024 embedded vector database library. It stores fixed-dimension
`f32` vectors and opaque payload bytes in one append-only file, uses exact
brute-force top-k search, and has no server process.

## Build, test, and lint commands

- `cargo build` - compile the crate.
- `cargo test` - run the full unit and integration test suite.
- `cargo test --test search_public_api` - run one integration test target;
  `db_public_api` is another integration target.
- `cargo test memory_put_get` - run tests matching a single name substring.
- `cargo run --example basic_search` - run an example; other examples are
  `filters` and `file_persistence`.
- `cargo fmt --check` - check formatting without modifying files.
- `cargo clippy --all-targets -- -D warnings` - fail on clippy warnings.

## Architecture

- `src/lib.rs` is the public API surface. It re-exports `EmveDb`, options,
  records, metrics, search types, and `EmveError`/`Result`.
- `EmveDb` in `src/db.rs` is the thread-safe facade. It wraps
  `RwLock<InnerDb>`, caches metadata (`dimension`, `metric`, open mode, max
  payload size), validates public inputs, and uses write locks for
  `put`/`delete`/`flush`/`compact` and read locks for `get`/`search`/`len`.
- `InnerDb` in `src/inner_db.rs` is the single-threaded core. It owns the
  header, storage wrapper, in-memory index, open mode, and sync mode; it
  re-validates inputs before appending frames and updates the index after
  storage writes.
- Storage is layered as `EmvedbStorage<S>` over the raw `Storage` trait. The
  wrapper understands the 64-byte header and length-prefixed frame stream, while
  `FileStorage` and `MemoryStorage` provide byte-level backends. File-backed
  opens use advisory locks: exclusive for read-write, shared for read-only.
- The append-only log is durable storage; `InMemoryIndex` is the source of truth
  for reads and search. Opening a database scans all frames with
  `read_all_frames()` and replays them with `InMemoryIndex::build_from_frames()`;
  later frames for the same id win, and delete frames are tombstones.
- `compact()` rewrites a fresh file containing only live records, increments the
  header generation, and swaps it in through `Storage::recreate`. File storage
  cleans stale `.compact` and `.compact.old` artifacts on open.
- The on-disk format is specified in `SPEC.md`: a CRC32C-protected 64-byte
  header followed by frames `[u32 len][body][u32 crc32c]`. Put frames store
  `id`, `payload_len`, vector bytes, and payload bytes; delete frames store
  only `id`.
- `FileStorage::validate()` distinguishes recoverable torn tail frames from
  non-recoverable middle corruption. Read-write opens truncate recoverable tails;
  read-only opens cap `logical_len` without modifying the file.
- Search results are always ordered by larger `score` first. L2 uses negative
  distance as score, cosine stores `inv_norm` in index entries, zero-norm cosine
  vectors score `-inf`, and score ties are broken by smaller id first.

## Key conventions

- Use `:memory:` for fast tests and temporary databases. It selects
  `MemoryStorage`, cannot be reopened with `open`, and compaction just swaps the
  in-memory buffer.
- Database dimensions must be `1..=65535`; vectors must contain only finite
  values; the only implemented element type is `F32`; payloads default to a
  16 MiB maximum.
- Public fallible APIs use the crate `Result<T>` alias and specific
  `EmveError` variants such as `DimensionMismatch`, `PayloadTooLarge`,
  `ReadOnly`, and `Locked`. Use `Corrupt` for integrity problems, not as a
  catch-all.
- Wrap fallible expressions whose errors should be traced with
  `with_debug_log!`; the crate uses `tracing`, and tests that assert tracing use
  `tracing-test`.
- Add black-box coverage for public behavior in `tests/`; keep focused unit
  tests next to implementation details. Prefer `:memory:` for fast behavior
  tests and `tempfile` for persistence, locking, compaction, reopen, and crash
  recovery scenarios.
- Keep `README.md`, `README_ja.md`, and `SPEC.md` aligned with public API or
  storage-format changes. Storage/frame/header/CRC/lock behavior changes should
  include persistence or recovery regression coverage.
- Do not commit generated `.emve` database files, `.compact` artifacts, local
  scratch data, or `target/` artifacts.
