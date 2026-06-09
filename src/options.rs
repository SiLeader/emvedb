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

pub const DEFAULT_MAX_PAYLOAD_LEN: usize = 16 * 1024 * 1024;

#[derive(Debug, Clone, Copy, Eq, PartialEq, Hash)]
pub enum OpenMode {
    ReadOnly,
    ReadWrite,
}

#[derive(Debug, Clone, Copy, Default, Eq, PartialEq, Hash)]
pub enum SyncMode {
    Always,
    #[default]
    OnFlush,
    Never,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct CreateOptions {
    pub dimension: u32,
    pub metric: Metric,
    pub sync: SyncMode,
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

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct OpenOptions {
    pub mode: OpenMode,
    pub lock_wait: bool,
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
