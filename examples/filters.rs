use emvedb::{CreateOptions, EmveDb, Metric, SearchOptions};

fn main() -> emvedb::Result<()> {
    let db = EmveDb::create(
        ":memory:",
        &CreateOptions {
            dimension: 3,
            metric: Metric::Dot,
            ..CreateOptions::default()
        },
    )?;

    let documents = [
        (100, [1.0, 1.0, 0.0], "rust-vector-intro"),
        (101, [0.0, 1.0, 1.0], "storage-layout"),
        (200, [1.0, 0.0, 1.0], "ops-runbook"),
        (201, [0.2, 1.0, 0.8], "vector-indexing"),
    ];

    for (id, vector, title) in documents {
        db.put(id, &vector, title.as_bytes())?;
    }

    let query = [1.0, 1.0, 0.0];
    let options = SearchOptions::default()
        .with_filter(|id| id < 200)
        .with_min_score(1.0);

    println!("id-filtered results:");
    for item in db.search(&query, 10, &options)? {
        let record = db.get(item.id)?.expect("search result should exist");
        println!(
            "id={} title={} score={:.3}",
            item.id,
            String::from_utf8_lossy(record.payload()),
            item.score
        );
    }

    let confident = SearchOptions::default().with_result_filter(|item| item.score >= 1.5);

    println!();
    println!("score-filtered results:");
    for item in db.search(&query, 10, &confident)? {
        let record = db.get(item.id)?.expect("search result should exist");
        println!(
            "id={} title={} score={:.3}",
            item.id,
            String::from_utf8_lossy(record.payload()),
            item.score
        );
    }

    Ok(())
}
