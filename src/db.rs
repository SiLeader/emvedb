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

use crate::inner_db::InnerDb;
use crate::options::DEFAULT_MAX_PAYLOAD_LEN;
use crate::record::Record;
use crate::search::{SearchOptions, SearchResultItem};
use crate::{CreateOptions, EmveError, Metric, OpenMode, OpenOptions};
use std::sync::RwLock;

/// A handle to an EmveDB database.
///
/// `EmveDb` stores fixed-dimension `f32` vectors keyed by `u64` ids, with an
/// opaque byte payload attached to each record. Handles are internally
/// synchronized, so methods take `&self`; write operations still require a
/// database opened in [`OpenMode::ReadWrite`].
pub struct EmveDb {
    meta: DbMeta,
    inner: RwLock<InnerDb>,
}

struct DbMeta {
    dimension: u32,
    metric: Metric,
    open_mode: OpenMode,
    max_payload_size: usize,
}

impl EmveDb {
    /// Creates a new database at `path`.
    ///
    /// Use `":memory:"` to create an in-memory database for tests or temporary
    /// indexes. File-backed creation fails if the file already exists.
    ///
    /// The requested dimension and metric are stored in the database header and
    /// must match all inserted vectors.
    pub fn create(path: &str, options: &CreateOptions) -> Result<Self, EmveError> {
        let inner = InnerDb::create(path, options)?;
        let meta = DbMeta {
            dimension: options.dimension,
            metric: options.metric,
            open_mode: OpenMode::ReadWrite,
            max_payload_size: options.max_payload_len,
        };
        Ok(Self {
            inner: RwLock::new(inner),
            meta,
        })
    }

    /// Opens an existing file-backed database.
    ///
    /// `":memory:"` databases cannot be reopened; create a new in-memory
    /// database instead. The returned handle uses the metric and dimension
    /// stored in the file.
    pub fn open(path: &str, options: &OpenOptions) -> Result<Self, EmveError> {
        let inner = InnerDb::open(path, options)?;
        let meta = DbMeta {
            dimension: inner.dimension(),
            metric: inner.metric(),
            open_mode: options.mode,
            max_payload_size: DEFAULT_MAX_PAYLOAD_LEN,
        };
        Ok(Self {
            inner: RwLock::new(inner),
            meta,
        })
    }

    /// Opens an existing database or creates a new one if no file exists.
    ///
    /// When opening an existing file, the metric and dimension are read from the
    /// file. When creating a new database, all values from `options` are used.
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
        if vector.len() != self.dimension() as usize {
            Err(EmveError::DimensionMismatch {
                expected: self.dimension(),
                got: vector.len() as u32,
            })
        } else {
            Ok(())
        }
    }
}

impl EmveDb {
    /// Inserts or replaces a record.
    ///
    /// `vector` must have exactly [`Self::dimension`] elements and every
    /// element must be finite. `payload` is stored as opaque bytes and is not
    /// interpreted by EmveDB.
    ///
    /// Returns [`EmveError::ReadOnly`] when called on a read-only handle.
    pub fn put(&self, id: u64, vector: &[f32], payload: &[u8]) -> Result<(), EmveError> {
        if self.meta.open_mode == OpenMode::ReadOnly {
            return Err(EmveError::ReadOnly);
        }

        // validata
        self.check_dimension(vector)?;
        if vector.iter().any(|x| !x.is_finite()) {
            return Err(EmveError::InvalidVector);
        }
        if payload.len() > self.meta.max_payload_size {
            return Err(EmveError::PayloadTooLarge {
                max: self.meta.max_payload_size,
                got: payload.len(),
            });
        }

        let mut inner = self.inner.write().map_err(|_| EmveError::LockFailed)?;
        inner.put(id, vector, payload)
    }

    /// Deletes the live record for `id`.
    ///
    /// Returns `true` if a live record existed and was removed, or `false` if
    /// the id was already absent.
    ///
    /// Returns [`EmveError::ReadOnly`] when called on a read-only handle.
    pub fn delete(&self, id: u64) -> Result<bool, EmveError> {
        if self.meta.open_mode == OpenMode::ReadOnly {
            return Err(EmveError::ReadOnly);
        }
        let mut inner = self.inner.write().map_err(|_| EmveError::LockFailed)?;
        inner.delete(id)
    }

    /// Returns the live record for `id`, if one exists.
    pub fn get(&self, id: u64) -> Result<Option<Record>, EmveError> {
        let inner = self.inner.read().map_err(|_| EmveError::LockFailed)?;
        inner.get(id)
    }

    /// Returns whether `id` currently has a live record.
    pub fn contains(&self, id: u64) -> Result<bool, EmveError> {
        let inner = self.inner.read().map_err(|_| EmveError::LockFailed)?;
        Ok(inner.contains(id))
    }

    /// Returns the number of live records.
    pub fn len(&self) -> Result<usize, EmveError> {
        let inner = self.inner.read().map_err(|_| EmveError::LockFailed)?;
        Ok(inner.len())
    }

    /// Returns whether the database contains no live records.
    pub fn is_empty(&self) -> Result<bool, EmveError> {
        let inner = self.inner.read().map_err(|_| EmveError::LockFailed)?;
        Ok(inner.is_empty())
    }

    /// Returns the metric used to score search results.
    pub fn metric(&self) -> Metric {
        self.meta.metric
    }

    /// Returns the fixed vector dimension for this database.
    pub fn dimension(&self) -> u32 {
        self.meta.dimension
    }

    /// Flushes pending storage changes according to the configured backend.
    ///
    /// With [`SyncMode::OnFlush`](crate::SyncMode::OnFlush), this asks the
    /// storage backend to sync data to durable storage.
    pub fn flush(&self) -> Result<(), EmveError> {
        let mut inner = self.inner.write().map_err(|_| EmveError::LockFailed)?;
        inner.flush()
    }
}

impl EmveDb {
    /// Rewrites storage so that only live records remain.
    ///
    /// Compaction can shrink append-only file-backed databases after updates
    /// and deletes. It is a write operation and returns [`EmveError::ReadOnly`]
    /// when called on a read-only handle.
    pub fn compact(&self) -> Result<(), EmveError> {
        if self.meta.open_mode == OpenMode::ReadOnly {
            return Err(EmveError::ReadOnly);
        }
        let mut inner = self.inner.write().map_err(|_| EmveError::LockFailed)?;
        inner.compact()
    }
}

impl EmveDb {
    /// Searches live records and returns the best `k` matches.
    ///
    /// Results are sorted best-first. [`SearchResultItem::score`] is always
    /// larger-is-better; [`SearchResultItem::distance`] is metric-specific.
    ///
    /// `query_vector` must match [`Self::dimension`] and contain only finite
    /// values. If `k` is `0`, an empty result set is returned without scanning.
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

        let inner = self.inner.read().map_err(|_| EmveError::LockFailed)?;
        inner.search(query_vector, k, options)
    }
}
