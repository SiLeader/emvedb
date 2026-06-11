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
use crate::index::InMemoryIndex;
use crate::options::DEFAULT_MAX_PAYLOAD_LEN;
use crate::record::Record;
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
            Self::open(path, &OpenOptions::default())
        } else {
            Self::create(path, options)
        }
    }
}

impl EmveDb {
    pub fn put(&mut self, id: u64, vector: &[f32], payload: &[u8]) -> Result<(), EmveError> {
        if self.open_mode == OpenMode::ReadOnly {
            return Err(EmveError::ReadOnly);
        }
        if vector.len() != self.index.dimension() as usize {
            return Err(EmveError::DimensionMismatch {
                expected: self.index.dimension(),
                got: vector.len() as u32,
            });
        }
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
        if !self.index.delete(id) {
            return Ok(false);
        }
        self.backend.append_frame(Frame::Delete { id })?;
        if self.sync_mode == SyncMode::Always {
            self.backend.sync()?;
        }
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

impl Drop for EmveDb {
    fn drop(&mut self) {
        if self.open_mode == OpenMode::ReadWrite {
            self.backend.sync().unwrap_or_else(|_| ());
        }
    }
}
