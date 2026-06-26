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

    pub fn delete(&self, id: u64) -> Result<bool, EmveError> {
        if self.meta.open_mode == OpenMode::ReadOnly {
            return Err(EmveError::ReadOnly);
        }
        let mut inner = self.inner.write().map_err(|_| EmveError::LockFailed)?;
        inner.delete(id)
    }

    pub fn get(&self, id: u64) -> Result<Option<Record>, EmveError> {
        let inner = self.inner.read().map_err(|_| EmveError::LockFailed)?;
        inner.get(id)
    }

    pub fn contains(&self, id: u64) -> Result<bool, EmveError> {
        let inner = self.inner.read().map_err(|_| EmveError::LockFailed)?;
        Ok(inner.contains(id))
    }

    pub fn len(&self) -> Result<usize, EmveError> {
        let inner = self.inner.read().map_err(|_| EmveError::LockFailed)?;
        Ok(inner.len())
    }

    pub fn is_empty(&self) -> Result<bool, EmveError> {
        let inner = self.inner.read().map_err(|_| EmveError::LockFailed)?;
        Ok(inner.is_empty())
    }

    pub fn metric(&self) -> Metric {
        self.meta.metric
    }

    pub fn dimension(&self) -> u32 {
        self.meta.dimension
    }

    pub fn flush(&self) -> Result<(), EmveError> {
        let mut inner = self.inner.write().map_err(|_| EmveError::LockFailed)?;
        inner.flush()
    }
}

impl EmveDb {
    pub fn compact(&self) -> Result<(), EmveError> {
        if self.meta.open_mode == OpenMode::ReadOnly {
            return Err(EmveError::ReadOnly);
        }
        let mut inner = self.inner.write().map_err(|_| EmveError::LockFailed)?;
        inner.compact()
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
