# EmveDB

EmveDB is an embedded, single-file vector database for Rust.

It stores fixed-dimension `f32` vectors and opaque payload bytes in one local
database file. It does not require a server process or external service. Search
is currently exact brute-force search over live records.

## Features

- Single-file storage for vectors and payloads
- Embedded Rust API centered on `EmveDb`
- `:memory:` mode for tests and temporary indexes
- Upsert, delete, get, exact top-k search, and compaction
- Metrics: cosine similarity, L2 distance, and dot product
- Append-only writes with CRC32C-framed records
- File locking for read-only and read-write opens
- Opaque payload bytes, so callers can store any serialized metadata

## Quick Start

When using this repository directly, add it to your `Cargo.toml`:

```toml
[dependencies]
emvedb = { path = "." }
```

Create a database, insert vectors, and search:

```rust
use emvedb::{CreateOptions, EmveDb, Metric, SearchOptions};

fn main() -> emvedb::Result<()> {
    let db = EmveDb::create(
        "example.emve",
        &CreateOptions {
            dimension: 3,
            metric: Metric::Cosine,
            ..CreateOptions::default()
        },
    )?;

    db.put(1, &[1.0, 0.0, 0.0], br#"{"label":"red"}"#)?;
    db.put(2, &[0.0, 1.0, 0.0], br#"{"label":"green"}"#)?;
    db.put(3, &[0.8, 0.2, 0.0], br#"{"label":"orange"}"#)?;

    let results = db.search(&[1.0, 0.0, 0.0], 2, &SearchOptions::default())?;

    for item in results {
        println!(
            "id={} score={} distance={}",
            item.id, item.score, item.distance
        );
    }

    db.flush()?;
    Ok(())
}
```

Use `:memory:` for tests and temporary indexes:

```rust
use emvedb::{CreateOptions, EmveDb, Metric};

let db = EmveDb::create(
    ":memory:",
    &CreateOptions {
        dimension: 2,
        metric: Metric::Dot,
        ..CreateOptions::default()
    },
)?;
```

## Data Model

Each record contains:

- `id`: a user-supplied `u64` primary key
- `vector`: a fixed-length `f32` slice matching the database dimension
- `payload`: arbitrary bytes; EmveDB does not parse or index the payload

`put` is an upsert. Writing the same `id` again logically replaces the live
value. `delete` logically removes the live record. Old versions and tombstones
remain in the append-only file until `compact()` rebuilds storage with only
live records.

Vector values must be finite. `NaN`, `+Inf`, and `-Inf` are rejected.

## Creating and Opening Databases

Use `create` for a new database, `open` for an existing database, or
`open_or_create` when either behavior is acceptable.

```rust
use emvedb::{EmveDb, OpenMode, OpenOptions};

let db = EmveDb::open(
    "example.emve",
    &OpenOptions {
        mode: OpenMode::ReadOnly,
        ..OpenOptions::default()
    },
)?;
```

Default options:

- `CreateOptions::default()` uses `Metric::Cosine`, `SyncMode::OnFlush`, and a
  16 MiB maximum payload length. You must set a valid `dimension`.
- `OpenOptions::default()` opens in `ReadWrite` mode, does not wait for a
  conflicting file lock, and uses `SyncMode::OnFlush`.

## Search

Search is exact top-k search over all live records.

```rust
use emvedb::SearchOptions;

let options = SearchOptions::default()
    .with_filter(|id| id != 42)
    .with_min_score(0.25);

let results = db.search(&query_vector, 10, &options)?;
```

Results are returned best-first. `SearchResultItem::score` is always a
"larger is better" value; `distance` is metric-specific.

| Metric | Ordering | `score` | `distance` |
| --- | --- | --- | --- |
| `Metric::Cosine` | higher cosine similarity first | cosine similarity | `1 - cosine` |
| `Metric::L2` | lower distance first | negative distance | Euclidean distance |
| `Metric::Dot` | higher dot product first | dot product | dot product |

Use `with_filter` to filter by id before scoring, or `with_result_filter` to
filter by computed search results.

## API Overview

```rust
EmveDb::create(path, &create_options)?;
EmveDb::open(path, &open_options)?;
EmveDb::open_or_create(path, &create_options)?;

db.put(id, vector, payload)?;
db.delete(id)?;
db.get(id)?;
db.contains(id)?;
db.search(query, k, &search_options)?;
db.len()?;
db.is_empty()?;
db.flush()?;
db.compact()?;

db.dimension();
db.metric();
```

`get` returns a `Record`. Use `record.id()`, `record.vector()`, and
`record.payload()` to access its fields.

## Persistence and Locking

File-backed databases keep data in a single `.emve` file. Writes append frames
to the end of the file, and `flush()` asks the storage backend to sync data.

`SyncMode` controls sync behavior:

- `Always`: sync after every `put` and `delete`
- `OnFlush`: sync on `flush()`; this is the default
- `Never`: leave syncing to the operating system

When opening a file, EmveDB uses advisory file locks:

- `ReadWrite` takes an exclusive lock
- `ReadOnly` takes a shared lock
- `lock_wait: false` returns `EmveError::Locked` immediately on conflict
- `lock_wait: true` waits for the lock

## Compaction

Because writes are append-only, updates and deletes can leave old data in the
file. Call `compact()` on a read-write database to rebuild storage using only
live records:

```rust
db.compact()?;
db.flush()?;
```

`compact()` is a write operation and is not available from `ReadOnly` handles.

## Limits

- Element type is currently `f32`
- Database dimension must be in `1..=65535`
- Payloads default to a maximum of 16 MiB per record
- Search is exact brute-force, `O(number_of_records * dimension)`
- `:memory:` databases cannot be reopened with `open`; create a new in-memory
  database instead

## Development

Run tests:

```sh
cargo test
```

Run format and lint checks:

```sh
cargo fmt --check
cargo clippy --workspace --all-targets --all-features
```

See [SPEC.md](SPEC.md) for the file format and detailed design notes.

## License

Licensed under the Apache License, Version 2.0. See [LICENSE](LICENSE).
