use emvedb::{CreateOptions, EmveDb, Metric, SearchOptions, SearchResultItem};

#[test]
fn search_types_are_usable_from_downstream_crates() {
    let mut db = EmveDb::create(
        ":memory:",
        &CreateOptions {
            dimension: 2,
            metric: Metric::Dot,
            ..CreateOptions::default()
        },
    )
    .unwrap();

    db.put(1, &[1.0, 0.0], b"one").unwrap();
    db.put(2, &[2.0, 0.0], b"two").unwrap();

    let options = SearchOptions::default()
        .with_filter(|id: u64| id == 2)
        .with_min_score(1.5);
    let results: Vec<SearchResultItem> = db.search(&[1.0, 0.0], 10, &options).unwrap();

    assert_eq!(results.len(), 1);
    assert_eq!(results[0].id, 2);
    assert_eq!(results[0].score, 2.0);
    assert_eq!(results[0].distance, 2.0);
}
