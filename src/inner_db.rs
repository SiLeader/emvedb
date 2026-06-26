use crate::format::{Frame, FrameRef, Header};
use crate::heap::BinaryTopKMinHeap;
use crate::index::InMemoryIndex;
use crate::metric::{
    compute_inv_norm, cos_score_and_distance, dot_score_and_distance, l2_score_and_distance,
};
use crate::options::DEFAULT_MAX_PAYLOAD_LEN;
use crate::storage::Storage;
use crate::storage::file::FileStorage;
use crate::storage::memory::MemoryStorage;
use crate::storage::storage::EmvedbStorage;
use crate::{
    CreateOptions, EmveError, Metric, OpenMode, OpenOptions, Record, SearchOptions,
    SearchResultItem, SyncMode,
};

pub(crate) struct InnerDb {
    backend: EmvedbStorage<Box<dyn Storage>>,
    header: Header,
    index: InMemoryIndex,
    open_mode: OpenMode,
    sync_mode: SyncMode,
    max_payload_size: usize,
}

impl InnerDb {
    pub fn create(path: &str, options: &CreateOptions) -> Result<Self, EmveError> {
        if !(1..=65535).contains(&options.dimension) {
            return Err(EmveError::DimensionOutOfRange {
                range: 1..=65535,
                got: options.dimension,
            });
        }
        let header = Header::initial_from_options(options);
        let storage = if path == ":memory:" {
            Box::new(MemoryStorage::new_empty(Header::initial_from_options(
                options,
            ))) as Box<dyn Storage>
        } else {
            if std::fs::exists(path)? {
                return Err(EmveError::AlreadyExists);
            }
            Box::new(FileStorage::create_new_from_options(path, options)?) as Box<dyn Storage>
        };
        let backend = EmvedbStorage::new(storage);
        let index =
            InMemoryIndex::build_from_frames(options.dimension, &backend.read_all_frames()?);

        Ok(Self {
            backend,
            index,
            header,
            open_mode: OpenMode::ReadWrite,
            sync_mode: options.sync,
            max_payload_size: options.max_payload_len,
        })
    }

    pub fn open(path: &str, options: &OpenOptions) -> Result<Self, EmveError> {
        if path == ":memory:" {
            return Err(EmveError::CannotOpenMemory);
        }
        let storage = if std::fs::exists(path)? {
            let fs = match options.mode {
                OpenMode::ReadOnly => FileStorage::open_readonly(path, options.lock_wait)?,
                OpenMode::ReadWrite => FileStorage::open_readwrite(path, options.lock_wait)?,
            };
            Box::new(fs) as Box<dyn Storage>
        } else {
            return Err(EmveError::FileNotFound(path.to_string()));
        };
        let backend = EmvedbStorage::new(storage);
        let header = backend.read_header()?;
        let index = InMemoryIndex::build_from_frames(header.dimension, &backend.read_all_frames()?);

        Ok(Self {
            backend,
            index,
            header,
            open_mode: options.mode,
            sync_mode: options.sync,
            max_payload_size: DEFAULT_MAX_PAYLOAD_LEN,
        })
    }
}

impl InnerDb {
    fn check_dimension(&self, vector: &[f32]) -> Result<(), EmveError> {
        if vector.len() != self.index.dimension() as usize {
            Err(EmveError::DimensionMismatch {
                expected: self.index.dimension(),
                got: vector.len() as u32,
            })
        } else {
            Ok(())
        }
    }
}

impl InnerDb {
    pub fn put(&mut self, id: u64, vector: &[f32], payload: &[u8]) -> Result<(), EmveError> {
        if self.open_mode == OpenMode::ReadOnly {
            return Err(EmveError::ReadOnly);
        }
        self.check_dimension(vector)?;
        if vector.iter().any(|x| !x.is_finite()) {
            return Err(EmveError::InvalidVector);
        }
        if payload.len() > self.max_payload_size {
            return Err(EmveError::PayloadTooLarge {
                max: self.max_payload_size,
                got: payload.len(),
            });
        }
        let offset = self.backend.append_frame(Frame::Put {
            id,
            vector,
            payload,
        })?;
        if self.sync_mode == SyncMode::Always {
            self.backend.sync()?;
        }
        let (_, frame) = self.backend.read_frame(offset)?;
        let FrameRef::Put {
            id,
            vector,
            payload_range,
        } = frame
        else {
            return Err(EmveError::Corrupt);
        };
        self.index.put(
            id,
            &vector,
            payload_range.start,
            (payload_range.end - payload_range.start) as u32,
        );
        Ok(())
    }

    pub fn delete(&mut self, id: u64) -> Result<bool, EmveError> {
        if self.open_mode == OpenMode::ReadOnly {
            return Err(EmveError::ReadOnly);
        }
        if !self.index.contains(id) {
            return Ok(false);
        }
        self.backend.append_frame(Frame::Delete { id })?;
        if self.sync_mode == SyncMode::Always {
            self.backend.sync()?;
        }
        self.index.delete(id);
        Ok(true)
    }

    pub fn get(&self, id: u64) -> Result<Option<Record>, EmveError> {
        let Some(entry) = self.index.get_entry(id) else {
            return Ok(None);
        };
        let vector = self.index.vector_of(entry.slot);
        let payload = self
            .backend
            .read_at(entry.payload_offset, entry.payload_len as usize)?;
        let record = Record::new(id, vector.to_vec(), payload);
        Ok(Some(record))
    }

    pub fn contains(&self, id: u64) -> bool {
        self.index.contains(id)
    }

    pub fn len(&self) -> usize {
        self.index.len()
    }

    pub fn is_empty(&self) -> bool {
        self.index.is_empty()
    }

    pub fn metric(&self) -> Metric {
        self.header.metric
    }

    pub fn dimension(&self) -> u32 {
        self.index.dimension()
    }

    pub fn flush(&mut self) -> Result<(), EmveError> {
        self.backend.sync()
    }
}

impl InnerDb {
    pub fn compact(&mut self) -> Result<(), EmveError> {
        if self.open_mode == OpenMode::ReadOnly {
            return Err(EmveError::ReadOnly);
        }
        let next_header = self.header.with_incremented_generation();

        let mut data = next_header.encode().to_vec();

        for (id, entry, vector) in self.index.iter_live() {
            let frame = Frame::Put {
                id,
                payload: &self
                    .backend
                    .read_at(entry.payload_offset, entry.payload_len as usize)?,
                vector,
            };
            data.extend(frame.encode());
        }

        self.backend.recreate(&data, "compact")?;
        self.header = next_header;
        self.index = InMemoryIndex::build_from_frames(
            self.header.dimension,
            &self.backend.read_all_frames()?,
        );
        Ok(())
    }
}

impl InnerDb {
    pub fn search(
        &self,
        query_vector: &[f32],
        k: usize,
        options: &SearchOptions,
    ) -> crate::Result<Vec<SearchResultItem>> {
        self.check_dimension(query_vector)?;
        if query_vector.iter().any(|x| !x.is_finite()) {
            return Err(EmveError::InvalidVector);
        }
        if k == 0 {
            return Ok(vec![]);
        }

        let inv_norm = if self.metric() == Metric::Cosine {
            Some(compute_inv_norm(query_vector))
        } else {
            None
        };

        let mut heap = BinaryTopKMinHeap::new(k);
        for (id, entry, vector) in self.index.iter_live() {
            if let Some(filter) = &options.filter
                && !filter.filter_id(id)
            {
                continue;
            }

            let (score, distance) = match self.metric() {
                Metric::Cosine => {
                    cos_score_and_distance(query_vector, inv_norm.unwrap(), vector, entry.inv_norm)
                }
                Metric::L2 => l2_score_and_distance(query_vector, vector),
                Metric::Dot => dot_score_and_distance(query_vector, vector),
            };
            if let Some(min_score) = options.min_score
                && score < min_score
            {
                continue;
            }
            let item = SearchResultItem {
                id,
                score,
                distance,
            };
            if let Some(filter) = &options.filter
                && !filter.filter(&item)
            {
                continue;
            }
            heap.push(item);
        }
        Ok(heap.into_vec())
    }
}

impl Drop for InnerDb {
    fn drop(&mut self) {
        if self.open_mode == OpenMode::ReadWrite {
            self.backend.sync().unwrap_or(());
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Record, SearchFilter, SearchOptions};

    fn options(dimension: u32) -> CreateOptions {
        CreateOptions {
            dimension,
            ..CreateOptions::default()
        }
    }

    fn options_with_metric(dimension: u32, metric: Metric) -> CreateOptions {
        CreateOptions {
            dimension,
            metric,
            ..CreateOptions::default()
        }
    }

    fn ids(results: &[SearchResultItem]) -> Vec<u64> {
        results.iter().map(|result| result.id).collect()
    }

    fn assert_approx_eq(actual: f32, expected: f32) {
        assert!(
            (actual - expected).abs() <= 1e-6,
            "expected {expected}, got {actual}"
        );
    }

    #[test]
    fn memory_put_get_update_delete() {
        let mut db = InnerDb::create(":memory:", &options(3)).unwrap();

        db.put(1, &[1.0, 2.0, 3.0], b"first").unwrap();
        let record: Record = db.get(1).unwrap().unwrap();
        assert_eq!(record.id(), 1);
        assert_eq!(record.vector(), &[1.0, 2.0, 3.0]);
        assert_eq!(record.payload(), b"first");
        assert_eq!(db.len(), 1);
        assert!(db.contains(1));

        db.put(1, &[4.0, 5.0, 6.0], b"second").unwrap();
        let record = db.get(1).unwrap().unwrap();
        assert_eq!(record.vector(), &[4.0, 5.0, 6.0]);
        assert_eq!(record.payload(), b"second");
        assert_eq!(db.len(), 1);

        assert!(db.delete(1).unwrap());
        assert!(!db.delete(1).unwrap());
        assert!(db.get(1).unwrap().is_none());
        assert!(db.is_empty());
    }

    #[test]
    fn file_reopen_preserves_live_records() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("db.emve");
        let path = path.to_str().unwrap();

        {
            let mut db = InnerDb::create(path, &options(2)).unwrap();
            db.put(1, &[1.0, 0.0], b"one").unwrap();
            db.put(2, &[0.0, 1.0], b"two").unwrap();
            db.put(1, &[0.5, 0.5], b"updated").unwrap();
            assert!(db.delete(2).unwrap());
            db.flush().unwrap();
        }

        let db = InnerDb::open(path, &OpenOptions::default()).unwrap();
        assert_eq!(db.len(), 1);
        assert!(db.get(2).unwrap().is_none());
        let record = db.get(1).unwrap().unwrap();
        assert_eq!(record.vector(), &[0.5, 0.5]);
        assert_eq!(record.payload(), b"updated");
    }

    #[test]
    fn readonly_put_and_delete_return_readonly_and_keep_index() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("db.emve");
        let path = path.to_str().unwrap();

        {
            let mut db = InnerDb::create(path, &options(2)).unwrap();
            db.put(7, &[1.0, 2.0], b"payload").unwrap();
        }

        let mut db = InnerDb::open(
            path,
            &OpenOptions {
                mode: OpenMode::ReadOnly,
                ..OpenOptions::default()
            },
        )
        .unwrap();

        assert!(matches!(
            db.put(8, &[3.0, 4.0], b"new"),
            Err(EmveError::ReadOnly)
        ));
        assert!(matches!(db.delete(7), Err(EmveError::ReadOnly)));
        let record = db.get(7).unwrap().unwrap();
        assert_eq!(record.payload(), b"payload");
        assert_eq!(db.len(), 1);
    }

    #[test]
    fn invalid_inputs_return_specific_errors() {
        let mut db = InnerDb::create(":memory:", &options(2)).unwrap();

        assert!(matches!(
            db.put(1, &[1.0], b"payload"),
            Err(EmveError::DimensionMismatch {
                expected: 2,
                got: 1
            })
        ));
        assert!(matches!(
            db.put(1, &[f32::NAN, 1.0], b"payload"),
            Err(EmveError::InvalidVector)
        ));

        let small_payload = CreateOptions {
            max_payload_len: 3,
            ..options(2)
        };
        let mut db = InnerDb::create(":memory:", &small_payload).unwrap();
        assert!(matches!(
            db.put(1, &[1.0, 2.0], b"toolong"),
            Err(EmveError::PayloadTooLarge { max: 3, got: 7 })
        ));
    }

    #[test]
    fn dot_search_returns_top_k_best_first_and_applies_filters() {
        let mut db = InnerDb::create(":memory:", &options_with_metric(2, Metric::Dot)).unwrap();
        db.put(1, &[1.0, 0.0], b"one").unwrap();
        db.put(2, &[3.0, 0.0], b"two").unwrap();
        db.put(3, &[2.0, 0.0], b"three").unwrap();
        db.put(4, &[4.0, 0.0], b"four").unwrap();

        let results = db
            .search(&[1.0, 0.0], 2, &SearchOptions::default())
            .unwrap();
        assert_eq!(ids(&results), vec![4, 2]);
        assert_eq!(results[0].score, 4.0);
        assert_eq!(results[0].distance, 4.0);

        let options = SearchOptions::default()
            .with_filter(|id| id != 4)
            .with_min_score(2.5);
        let results = db.search(&[1.0, 0.0], 10, &options).unwrap();
        assert_eq!(ids(&results), vec![2]);
    }

    #[test]
    fn search_filter_can_inspect_scored_result() {
        struct ScoreAtMost(f32);

        impl SearchFilter for ScoreAtMost {
            fn filter(&self, result: &SearchResultItem) -> bool {
                result.score <= self.0
            }
        }

        let mut db = InnerDb::create(":memory:", &options_with_metric(2, Metric::Dot)).unwrap();
        db.put(1, &[1.0, 0.0], b"one").unwrap();
        db.put(2, &[2.0, 0.0], b"two").unwrap();
        db.put(3, &[3.0, 0.0], b"three").unwrap();

        let options = SearchOptions::default().with_filter(ScoreAtMost(2.0));
        let results = db.search(&[1.0, 0.0], 10, &options).unwrap();

        assert_eq!(ids(&results), vec![2, 1]);
    }

    #[test]
    fn cosine_search_handles_same_direction_and_zero_norm() {
        let mut db = InnerDb::create(":memory:", &options_with_metric(2, Metric::Cosine)).unwrap();
        db.put(2, &[100.0, 0.0], b"same-long").unwrap();
        db.put(1, &[1.0, 0.0], b"same").unwrap();
        db.put(3, &[0.0, 1.0], b"orthogonal").unwrap();
        db.put(4, &[0.0, 0.0], b"zero").unwrap();

        let results = db
            .search(&[1.0, 0.0], 4, &SearchOptions::default())
            .unwrap();

        assert_eq!(ids(&results), vec![1, 2, 3, 4]);
        assert_approx_eq(results[0].score, 1.0);
        assert_approx_eq(results[1].score, 1.0);
        assert_approx_eq(results[0].distance, 0.0);
        assert_approx_eq(results[2].score, 0.0);
        assert!(results[3].score.is_infinite() && results[3].score.is_sign_negative());
        assert!(results[3].distance.is_infinite() && results[3].distance.is_sign_positive());
    }

    #[test]
    fn l2_search_returns_euclidean_distance_and_handles_boundaries() {
        let mut db = InnerDb::create(":memory:", &options_with_metric(2, Metric::L2)).unwrap();
        db.put(1, &[0.0, 0.0], b"origin").unwrap();
        db.put(2, &[3.0, 4.0], b"far").unwrap();
        db.put(3, &[1.0, 0.0], b"near").unwrap();

        let empty = db
            .search(&[0.0, 0.0], 0, &SearchOptions::default())
            .unwrap();
        assert!(empty.is_empty());

        let results = db
            .search(&[0.0, 0.0], 10, &SearchOptions::default())
            .unwrap();
        assert_eq!(ids(&results), vec![1, 3, 2]);
        assert_approx_eq(results[0].distance, 0.0);
        assert_approx_eq(results[1].distance, 1.0);
        assert_approx_eq(results[2].distance, 5.0);
        assert_approx_eq(results[2].score, -5.0);
    }

    #[test]
    fn search_validates_query_and_orders_ties_by_id() {
        let mut db = InnerDb::create(":memory:", &options_with_metric(2, Metric::Dot)).unwrap();
        db.put(2, &[1.0, 0.0], b"two").unwrap();
        db.put(1, &[1.0, 0.0], b"one").unwrap();

        let results = db
            .search(&[1.0, 0.0], 2, &SearchOptions::default())
            .unwrap();
        assert_eq!(ids(&results), vec![1, 2]);

        assert!(matches!(
            db.search(&[1.0], 2, &SearchOptions::default()),
            Err(EmveError::DimensionMismatch {
                expected: 2,
                got: 1
            })
        ));
        assert!(matches!(
            db.search(&[f32::NAN, 0.0], 2, &SearchOptions::default()),
            Err(EmveError::InvalidVector)
        ));
    }
}
