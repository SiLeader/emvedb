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

use crate::format::{FrameError, HeaderError};
use thiserror::Error;

/// Error type returned by EmveDB operations.
#[derive(Debug, Error)]
pub enum EmveError {
    /// Underlying I/O operation failed.
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),
    /// Stored data is truncated, inconsistent, or otherwise corrupt.
    #[error("Frame corrupted")]
    Corrupt,
    /// A frame in the append-only log is invalid.
    #[error("Invalid frame: {0}")]
    InvalidFrame(FrameError),
    /// The database header is invalid.
    #[error("Invalid header: {0}")]
    InvalidHeader(HeaderError),
    /// The database file was written by an unsupported format version.
    #[error("Unsupported version")]
    UnsupportedVersion,
    /// The database file does not start with the expected magic bytes.
    #[error("Invalid magic")]
    InvalidMagic,
    /// A vector did not match the database dimension.
    #[error("Vector dimension mismatch (expected {expected}, got {got})")]
    DimensionMismatch {
        /// Dimension required by the database.
        expected: u32,
        /// Dimension supplied by the caller.
        got: u32,
    },
    /// A vector contained `NaN`, positive infinity, or negative infinity.
    #[error("Invalid vector")]
    InvalidVector,
    /// A payload exceeded the configured maximum length.
    #[error("Payload too large (max {max} bytes, got {got} bytes)")]
    PayloadTooLarge {
        /// Maximum allowed payload length in bytes.
        max: usize,
        /// Payload length supplied by the caller in bytes.
        got: usize,
    },
    /// A file lock could not be acquired immediately.
    #[error("Locked")]
    Locked,
    /// A file-backed database already exists at the requested create path.
    #[error("Already exists")]
    AlreadyExists,
    /// A write operation was attempted through a read-only handle.
    #[error("Read only")]
    ReadOnly,
    /// An internal storage mutex was poisoned.
    #[error("Internal mutex")]
    InternalLock,
    /// The requested database dimension is outside the supported range.
    #[error("Dimension out of range: {range:?} (got {got})")]
    DimensionOutOfRange {
        /// Supported inclusive dimension range.
        range: std::ops::RangeInclusive<u32>,
        /// Dimension supplied by the caller.
        got: u32,
    },
    /// Attempted to open `":memory:"` with [`EmveDb::open`](crate::EmveDb::open).
    #[error("Cannot open :memory: storage")]
    CannotOpenMemory,
    /// The requested database file does not exist.
    #[error("File not found: {0}")]
    FileNotFound(String),
    /// The database handle lock was poisoned.
    #[error("Lock error")]
    LockFailed,
}

/// Crate-wide result type.
pub type Result<T> = std::result::Result<T, EmveError>;

/// Logs an error at debug level while preserving the original result.
#[macro_export]
macro_rules! with_debug_log {
    ($ex:expr) => {
        $ex.inspect_err(|e| tracing::debug!("error at line {}: {:?}", line!(), e))
    };
}
