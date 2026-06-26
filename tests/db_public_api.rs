use emvedb::{CreateOptions, EmveDb, EmveError, Metric, OpenMode, OpenOptions, SearchOptions};
use std::cmp::Ordering;
use std::fs;
use std::io::Write;
use std::path::Path;

fn path_string(path: &Path) -> String {
    path.to_str().unwrap().to_string()
}

fn create_options(dimension: u32, metric: Metric) -> CreateOptions {
    CreateOptions {
        dimension,
        metric,
        ..CreateOptions::default()
    }
}

fn ids(results: &[emvedb::SearchResultItem]) -> Vec<u64> {
    results.iter().map(|result| result.id).collect()
}

fn assert_record(db: &EmveDb, id: u64, vector: &[f32], payload: &[u8]) {
    let record = db.get(id).unwrap().unwrap();
    assert_eq!(record.id(), id);
    assert_eq!(record.vector(), vector);
    assert_eq!(record.payload(), payload);
}

fn run_crud_roundtrip(path: &str) {
    let db = EmveDb::create(path, &create_options(3, Metric::Dot)).unwrap();
    assert_eq!(db.dimension(), 3);
    assert_eq!(db.metric(), Metric::Dot);
    assert!(db.is_empty().unwrap());

    db.put(1, &[1.0, 0.0, 0.0], b"one").unwrap();
    db.put(2, &[0.0, 2.0, 0.0], b"two").unwrap();
    db.put(3, &[0.0, 0.0, 3.0], b"three").unwrap();
    assert_eq!(db.len().unwrap(), 3);
    assert!(db.contains(2).unwrap());
    assert_record(&db, 2, &[0.0, 2.0, 0.0], b"two");

    db.put(2, &[5.0, 0.0, 0.0], b"two-updated").unwrap();
    assert_eq!(db.len().unwrap(), 3);
    assert_record(&db, 2, &[5.0, 0.0, 0.0], b"two-updated");

    assert!(db.delete(1).unwrap());
    assert!(!db.delete(1).unwrap());
    assert!(db.get(1).unwrap().is_none());
    assert_eq!(db.len().unwrap(), 2);

    let results = db
        .search(&[1.0, 0.0, 0.0], 10, &SearchOptions::default())
        .unwrap();
    assert_eq!(ids(&results), vec![2, 3]);
}

#[test]
fn crud_roundtrip_uses_same_flow_for_memory_and_file() {
    run_crud_roundtrip(":memory:");

    let dir = tempfile::tempdir().unwrap();
    let path = path_string(&dir.path().join("roundtrip.emve"));
    run_crud_roundtrip(&path);
}

#[test]
fn reopen_preserves_live_records_and_metadata() {
    let dir = tempfile::tempdir().unwrap();
    let path = path_string(&dir.path().join("reopen.emve"));

    {
        let db = EmveDb::create(&path, &create_options(2, Metric::L2)).unwrap();
        db.put(1, &[0.0, 0.0], b"old").unwrap();
        db.put(2, &[3.0, 4.0], b"deleted").unwrap();
        db.put(1, &[1.0, 0.0], b"new").unwrap();
        assert!(db.delete(2).unwrap());
        db.flush().unwrap();
    }

    let db = EmveDb::open(&path, &OpenOptions::default()).unwrap();
    assert_eq!(db.dimension(), 2);
    assert_eq!(db.metric(), Metric::L2);
    assert_eq!(db.len().unwrap(), 1);
    assert_record(&db, 1, &[1.0, 0.0], b"new");
    assert!(db.get(2).unwrap().is_none());
}

#[test]
fn compact_preserves_live_data_shrinks_file_and_removes_artifacts() {
    let dir = tempfile::tempdir().unwrap();
    let path_buf = dir.path().join("compact.emve");
    let path = path_string(&path_buf);
    let compact_path = format!("{path}.compact");
    let old_path = format!("{path}.compact.old");

    let db = EmveDb::create(&path, &create_options(2, Metric::Dot)).unwrap();
    for i in 0..30 {
        let payload = vec![b'a' + (i % 20) as u8; 512];
        db.put(1, &[i as f32, 0.0], &payload).unwrap();
    }
    db.put(2, &[0.0, 1.0], &[b'x'; 1024]).unwrap();
    db.delete(2).unwrap();
    db.put(3, &[0.0, 2.0], b"three").unwrap();
    db.flush().unwrap();

    let before_generation = generation(&path_buf);
    let before_size = fs::metadata(&path_buf).unwrap().len();
    let before_results = db
        .search(&[1.0, 0.0], 10, &SearchOptions::default())
        .unwrap();

    db.compact().unwrap();
    db.flush().unwrap();

    let after_size = fs::metadata(&path_buf).unwrap().len();
    let after_results = db
        .search(&[1.0, 0.0], 10, &SearchOptions::default())
        .unwrap();

    assert!(after_size < before_size);
    assert_eq!(generation(&path_buf), before_generation.wrapping_add(1));
    assert_eq!(ids(&after_results), ids(&before_results));
    assert_record(&db, 1, &[29.0, 0.0], &vec![b'a' + 9; 512]);
    assert_record(&db, 3, &[0.0, 2.0], b"three");
    assert!(!Path::new(&compact_path).exists());
    assert!(!Path::new(&old_path).exists());
}

#[test]
fn stale_compact_artifacts_are_removed_on_open() {
    let dir = tempfile::tempdir().unwrap();
    let path = path_string(&dir.path().join("stale-artifact.emve"));
    {
        let db = EmveDb::create(&path, &create_options(2, Metric::Cosine)).unwrap();
        db.put(1, &[1.0, 0.0], b"one").unwrap();
    }
    let compact_path = format!("{path}.compact");
    let old_path = format!("{path}.compact.old");
    fs::write(&compact_path, b"stale temp").unwrap();
    fs::write(&old_path, b"stale old").unwrap();

    let db = EmveDb::open(&path, &OpenOptions::default()).unwrap();

    assert_record(&db, 1, &[1.0, 0.0], b"one");
    assert!(!Path::new(&compact_path).exists());
    assert!(!Path::new(&old_path).exists());
}

#[test]
fn readonly_open_ignores_torn_tail_without_truncating_then_rw_recovers() {
    let dir = tempfile::tempdir().unwrap();
    let path = path_string(&dir.path().join("torn-readonly.emve"));
    {
        let db = EmveDb::create(&path, &create_options(2, Metric::Cosine)).unwrap();
        db.put(1, &[1.0, 0.0], b"one").unwrap();
        db.flush().unwrap();
    }
    let valid_len = fs::metadata(&path).unwrap().len();
    {
        let mut raw = fs::OpenOptions::new().append(true).open(&path).unwrap();
        raw.write_all(&[0xab, 0xcd]).unwrap();
    }
    let torn_len = fs::metadata(&path).unwrap().len();
    assert!(torn_len > valid_len);

    {
        let db = EmveDb::open(
            &path,
            &OpenOptions {
                mode: OpenMode::ReadOnly,
                ..OpenOptions::default()
            },
        )
        .unwrap();
        assert_record(&db, 1, &[1.0, 0.0], b"one");
    }
    assert_eq!(fs::metadata(&path).unwrap().len(), torn_len);

    let db = EmveDb::open(&path, &OpenOptions::default()).unwrap();
    assert_record(&db, 1, &[1.0, 0.0], b"one");
    drop(db);
    assert_eq!(fs::metadata(&path).unwrap().len(), valid_len);
}

#[test]
fn public_error_paths_are_specific() {
    let dir = tempfile::tempdir().unwrap();
    let missing = path_string(&dir.path().join("missing.emve"));
    assert!(matches!(
        EmveDb::open(&missing, &OpenOptions::default()),
        Err(EmveError::FileNotFound(path)) if path == missing
    ));
    assert!(matches!(
        EmveDb::open(":memory:", &OpenOptions::default()),
        Err(EmveError::CannotOpenMemory)
    ));
    assert!(matches!(
        EmveDb::create(":memory:", &create_options(0, Metric::Cosine)),
        Err(EmveError::DimensionOutOfRange { got: 0, .. })
    ));

    let path = path_string(&dir.path().join("errors.emve"));
    let db = EmveDb::create(
        &path,
        &CreateOptions {
            dimension: 2,
            max_payload_len: 3,
            ..CreateOptions::default()
        },
    )
    .unwrap();

    assert!(matches!(
        EmveDb::create(&path, &create_options(2, Metric::Cosine)),
        Err(EmveError::AlreadyExists)
    ));
    assert!(matches!(
        EmveDb::open(&path, &OpenOptions::default()),
        Err(EmveError::Locked)
    ));
    assert!(matches!(
        db.put(1, &[1.0], b"ok"),
        Err(EmveError::DimensionMismatch {
            expected: 2,
            got: 1
        })
    ));
    assert!(matches!(
        db.put(1, &[f32::INFINITY, 0.0], b"ok"),
        Err(EmveError::InvalidVector)
    ));
    assert!(matches!(
        db.put(1, &[1.0, 0.0], b"toolong"),
        Err(EmveError::PayloadTooLarge { max: 3, got: 7 })
    ));
    assert!(matches!(
        db.search(&[f32::NAN, 0.0], 1, &SearchOptions::default()),
        Err(EmveError::InvalidVector)
    ));
    drop(db);

    let ro = EmveDb::open(
        &path,
        &OpenOptions {
            mode: OpenMode::ReadOnly,
            ..OpenOptions::default()
        },
    )
    .unwrap();
    assert!(matches!(
        ro.put(1, &[1.0, 0.0], b"ok"),
        Err(EmveError::ReadOnly)
    ));
    assert!(matches!(ro.delete(1), Err(EmveError::ReadOnly)));
    assert!(matches!(ro.compact(), Err(EmveError::ReadOnly)));
}

#[test]
fn search_matches_reference_sort_for_each_metric() {
    for metric in [Metric::Cosine, Metric::L2, Metric::Dot] {
        let db = EmveDb::create(":memory:", &create_options(2, metric)).unwrap();
        let rows = [
            (1, [1.0, 0.0]),
            (2, [0.0, 1.0]),
            (3, [2.0, 0.0]),
            (4, [-1.0, 0.0]),
            (5, [0.0, 0.0]),
        ];
        for (id, vector) in rows {
            db.put(id, &vector, &[]).unwrap();
        }

        let query = [1.0, 0.0];
        let results = db.search(&query, 4, &SearchOptions::default()).unwrap();
        let expected = reference_ids(metric, &query, &rows, 4);
        assert_eq!(ids(&results), expected);
    }
}

fn generation(path: &Path) -> u32 {
    let bytes = fs::read(path).unwrap();
    u32::from_le_bytes(bytes[10..14].try_into().unwrap())
}

fn reference_ids(metric: Metric, query: &[f32; 2], rows: &[(u64, [f32; 2])], k: usize) -> Vec<u64> {
    let mut scored: Vec<(u64, f32)> = rows
        .iter()
        .map(|(id, vector)| (*id, reference_score(metric, query, vector)))
        .collect();
    scored.sort_by(|(left_id, left_score), (right_id, right_score)| {
        match right_score.total_cmp(left_score) {
            Ordering::Equal => left_id.cmp(right_id),
            ordering => ordering,
        }
    });
    scored.into_iter().take(k).map(|(id, _)| id).collect()
}

fn reference_score(metric: Metric, query: &[f32; 2], vector: &[f32; 2]) -> f32 {
    match metric {
        Metric::Cosine => {
            let query_norm = (query[0] * query[0] + query[1] * query[1]).sqrt();
            let vector_norm = (vector[0] * vector[0] + vector[1] * vector[1]).sqrt();
            if query_norm == 0.0 || vector_norm == 0.0 {
                f32::NEG_INFINITY
            } else {
                (query[0] * vector[0] + query[1] * vector[1]) / query_norm / vector_norm
            }
        }
        Metric::L2 => {
            let dx = query[0] - vector[0];
            let dy = query[1] - vector[1];
            -(dx * dx + dy * dy).sqrt()
        }
        Metric::Dot => query[0] * vector[0] + query[1] * vector[1],
    }
}
