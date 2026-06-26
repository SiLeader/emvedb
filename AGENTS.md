# Repository Guidelines

## Project Structure & Module Organization

EmveDB is a Rust 2024 library crate. `src/lib.rs` defines the public surface by
re-exporting `EmveDb`, options, records, metrics, search types, and errors. Core
database behavior lives in `src/db.rs`, `src/inner_db.rs`, and `src/index.rs`.
The append-only file format is under `src/format/`, while storage backends and
locking are under `src/storage/`. Integration tests are in `tests/`; runnable
examples are in `examples/`. Keep `README.md`, `README_ja.md`, and `SPEC.md`
aligned with public API or storage-format changes.

## Build, Test, and Development Commands

- `cargo build`: compile the crate.
- `cargo test`: run the full test suite.
- `cargo test --test search_public_api`: run one integration test target.
- `cargo run --example basic_search`: run a documented example. Other examples
  include `filters` and `file_persistence`.
- `cargo fmt --check`: verify Rust formatting without changing files.
- `cargo clippy --all-targets -- -D warnings`: run lint checks and fail on
  warnings.

## Coding Style & Naming Conventions

Use standard `rustfmt` formatting with 4-space indentation. Name modules,
functions, variables, and test functions in `snake_case`; name public structs,
enums, and traits in `PascalCase`. Prefer the crate `Result<T>` alias and
`EmveError` for fallible public APIs. Keep public API names consistent with the
existing domain vocabulary: records, metrics, frames, storage, search, and
options.

## Testing Guidelines

Add integration tests in `tests/` for public behavior and focused unit tests
near implementation details when useful. Prefer `:memory:` databases for fast
tests and `tempfile` for file-backed persistence, locking, compaction, or
reopen behavior. Test names should describe the behavior being checked, for
example `search_rejects_wrong_dimension`. There is no formal coverage threshold;
new storage, search, and error-path changes should include regression coverage.

## Commit & Pull Request Guidelines

Recent commits use concise, present-tense subjects with capitalized verbs, such
as `Add search filtering capabilities` or `Enhance FileStorage...`. Keep commits
scoped to one logical change and include docs or tests in the same commit when
they are part of the behavior. Pull requests should summarize behavior changes,
call out storage-format or compatibility impacts, link related issues, and list
the validation commands run, such as `cargo test` and `cargo clippy`.

## Storage & Configuration Notes

Do not commit generated `.emve` database files, local scratch data, or `target/`
artifacts. Be careful when changing frame encoding, CRC handling, lock behavior,
or sync modes; update `SPEC.md` and persistence tests with those changes.
