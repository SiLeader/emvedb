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

use crate::Metric;

/// Default maximum payload length for newly created databases.
pub const DEFAULT_MAX_PAYLOAD_LEN: usize = 16 * 1024 * 1024;

/// Access mode used when opening an existing database.
#[derive(Debug, Clone, Copy, Eq, PartialEq, Hash)]
pub enum OpenMode {
    /// Open for reads only.
    ReadOnly,
    /// Open for reads and writes.
    ReadWrite,
}

/// Controls when writes are synchronized to durable storage.
#[derive(Debug, Clone, Copy, Default, Eq, PartialEq, Hash)]
pub enum SyncMode {
    /// Sync after every write operation.
    Always,
    /// Sync when [`EmveDb::flush`](crate::EmveDb::flush) is called.
    #[default]
    OnFlush,
    /// Do not explicitly sync; leave persistence timing to the operating system.
    Never,
}

/// Options used when creating a database.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct CreateOptions {
    /// Fixed vector dimension for every record.
    ///
    /// Must be in `1..=65535`.
    pub dimension: u32,
    /// Metric used to rank search results.
    pub metric: Metric,
    /// Storage synchronization policy.
    pub sync: SyncMode,
    /// Maximum payload size, in bytes, accepted by [`EmveDb::put`](crate::EmveDb::put).
    pub max_payload_len: usize,
}

impl Default for CreateOptions {
    fn default() -> Self {
        Self {
            dimension: 0,
            metric: Metric::Cosine,
            sync: SyncMode::default(),
            max_payload_len: DEFAULT_MAX_PAYLOAD_LEN,
        }
    }
}

/// Options used when opening an existing database.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct OpenOptions {
    /// Access mode for the returned database handle.
    pub mode: OpenMode,
    /// Whether to wait for a conflicting file lock.
    ///
    /// If `false`, opening returns [`EmveError::Locked`](crate::EmveError::Locked)
    /// immediately when another handle holds an incompatible lock.
    pub lock_wait: bool,
    /// Storage synchronization policy for write-capable handles.
    pub sync: SyncMode,
}

impl Default for OpenOptions {
    fn default() -> Self {
        Self {
            mode: OpenMode::ReadWrite,
            lock_wait: false,
            sync: SyncMode::default(),
        }
    }
}
