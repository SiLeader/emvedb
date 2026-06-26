use emvedb::{CreateOptions, EmveDb, Metric, SearchOptions};

fn main() -> emvedb::Result<()> {
    let db = EmveDb::create(
        ":memory:",
        &CreateOptions {
            dimension: 3,
            metric: Metric::Cosine,
            ..CreateOptions::default()
        },
    )?;

    let colors = [
        (1, [1.0, 0.0, 0.0], "red"),
        (2, [0.0, 1.0, 0.0], "green"),
        (3, [0.0, 0.0, 1.0], "blue"),
        (4, [0.8, 0.2, 0.0], "orange"),
    ];

    for (id, vector, label) in colors {
        db.put(id, &vector, label.as_bytes())?;
    }

    let query = [1.0, 0.1, 0.0];
    let results = db.search(&query, 3, &SearchOptions::default())?;

    for item in results {
        let record = db.get(item.id)?.expect("search result should exist");
        let label = String::from_utf8_lossy(record.payload());
        println!(
            "id={} label={} score={:.3} distance={:.3}",
            item.id, label, item.score, item.distance
        );
    }

    Ok(())
}
