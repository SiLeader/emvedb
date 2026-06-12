// Copyright 2026- SiLeader (Cerussite).
//
// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at
//
//     http://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.

use crate::format::{Frame, FrameRef, Header};
use crate::heap::BinaryTopKMinHeap;
use crate::index::InMemoryIndex;
use crate::metric::{compute_inv_norm, cos_scoring, dot_scoring, l2_scoring};
use crate::options::DEFAULT_MAX_PAYLOAD_LEN;
use crate::record::Record;
use crate::search::{SearchOptions, SearchResultItem};
use crate::storage::Storage;
use crate::storage::file::FileStorage;
use crate::storage::memory::MemoryStorage;
use crate::storage::storage::EmvedbStorage;
use crate::{CreateOptions, EmveError, Metric, OpenMode, OpenOptions, SyncMode};

pub struct EmveDb {
    backend: EmvedbStorage<Box<dyn Storage>>,
    header: Header,
    index: InMemoryIndex,
    open_mode: OpenMode,
    sync_mode: SyncMode,
    max_payload_size: usize,
}

impl EmveDb {
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

    pub fn open_or_create(path: &str, options: &CreateOptions) -> Result<Self, EmveError> {
        if std::fs::exists(path)? {
            Self::open(
                path,
                &OpenOptions {
                    sync: options.sync,
                    ..OpenOptions::default()
                },
            )
        } else {
            Self::create(path, options)
        }
    }
}

impl EmveDb {
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

impl EmveDb {
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

impl EmveDb {
    pub fn search(
        &self,
        query_vector: &[f32],
        k: usize,
        options: &SearchOptions,
    ) -> crate::Result<Vec<SearchResultItem>> {
        self.check_dimension(query_vector)?;
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
            let score = match self.metric() {
                Metric::Cosine => {
                    cos_scoring(query_vector, inv_norm.unwrap(), vector, entry.inv_norm)
                }
                Metric::L2 => l2_scoring(query_vector, vector),
                Metric::Dot => dot_scoring(query_vector, vector),
            };
            if let Some(min_score) = options.min_score {
                if score < min_score {
                    continue;
                }
            }
            let item = SearchResultItem { id, score };
            if let Some(filter) = &options.filter {
                if !filter.filter(&item) {
                    continue;
                }
            }
            heap.push(item);
        }
        Ok(heap.into_vec())
    }
}

impl Drop for EmveDb {
    fn drop(&mut self) {
        if self.open_mode == OpenMode::ReadWrite {
            self.backend.sync().unwrap_or_else(|_| ());
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Record;

    fn options(dimension: u32) -> CreateOptions {
        CreateOptions {
            dimension,
            ..CreateOptions::default()
        }
    }

    #[test]
    fn memory_put_get_update_delete() {
        let mut db = EmveDb::create(":memory:", &options(3)).unwrap();

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
            let mut db = EmveDb::create(path, &options(2)).unwrap();
            db.put(1, &[1.0, 0.0], b"one").unwrap();
            db.put(2, &[0.0, 1.0], b"two").unwrap();
            db.put(1, &[0.5, 0.5], b"updated").unwrap();
            assert!(db.delete(2).unwrap());
            db.flush().unwrap();
        }

        let db = EmveDb::open(path, &OpenOptions::default()).unwrap();
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
            let mut db = EmveDb::create(path, &options(2)).unwrap();
            db.put(7, &[1.0, 2.0], b"payload").unwrap();
        }

        let mut db = EmveDb::open(
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
        let mut db = EmveDb::create(":memory:", &options(2)).unwrap();

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
        let mut db = EmveDb::create(":memory:", &small_payload).unwrap();
        assert!(matches!(
            db.put(1, &[1.0, 2.0], b"toolong"),
            Err(EmveError::PayloadTooLarge { max: 3, got: 7 })
        ));
    }
}
