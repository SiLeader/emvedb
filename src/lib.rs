//! Embedded, single-file vector database.
//!
//! EmveDB stores fixed-dimension `f32` vectors and opaque payload bytes in an
//! append-only file. The public API is built around [`Db`], [`Metric`],
//! [`CreateOptions`], and [`OpenOptions`].

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

mod db;
pub mod element_type;
mod error;
mod format;
mod index;
mod metric;
mod options;
mod record;
mod search;
mod storage;

pub use crate::db::EmveDb;
pub use crate::error::{EmveError, Result};
pub use crate::metric::Metric;
pub use crate::options::{CreateOptions, OpenMode, OpenOptions, SyncMode};
pub use crate::record::Record;
