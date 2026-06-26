use emvedb::{CreateOptions, EmveDb, Metric, OpenMode, OpenOptions, SearchOptions};
use std::fs;
use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

fn main() -> emvedb::Result<()> {
    let path = example_path();
    let path_string = path.to_string_lossy().into_owned();

    {
        let db = EmveDb::create(
            &path_string,
            &CreateOptions {
                dimension: 2,
                metric: Metric::L2,
                ..CreateOptions::default()
            },
        )?;

        db.put(1, &[0.0, 0.0], b"old-origin")?;
        db.put(2, &[5.0, 5.0], b"deleted")?;
        db.put(1, &[1.0, 0.0], b"origin")?;
        db.put(3, &[2.0, 1.0], b"nearby")?;
        db.delete(2)?;
        db.compact()?;
        db.flush()?;
    }

    {
        let db = EmveDb::open(
            &path_string,
            &OpenOptions {
                mode: OpenMode::ReadOnly,
                ..OpenOptions::default()
            },
        )?;

        println!(
            "opened {} with dimension={} metric={:?} len={}",
            path.display(),
            db.dimension(),
            db.metric(),
            db.len()?
        );

        for item in db.search(&[1.0, 1.0], 10, &SearchOptions::default())? {
            let record = db.get(item.id)?.expect("search result should exist");
            println!(
                "id={} payload={} distance={:.3}",
                item.id,
                String::from_utf8_lossy(record.payload()),
                item.distance
            );
        }
    }

    fs::remove_file(path)?;

    Ok(())
}

fn example_path() -> PathBuf {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos();
    std::env::temp_dir().join(format!("emvedb-example-{nanos}.emve"))
}
