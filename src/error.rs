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
use tracing::Level;

#[derive(Debug, Error)]
pub enum EmveError {
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),
    #[error("Frame corrupted")]
    Corrupt,
    #[error("Invalid frame: {0}")]
    InvalidFrame(FrameError),
    #[error("Invalid header: {0}")]
    InvalidHeader(HeaderError),
    #[error("Unsupported version")]
    UnsupportedVersion,
    #[error("Invalid magic")]
    InvalidMagic,
    #[error("Vector dimension mismatch (expected {0}, got {1})")]
    DimensionMismatch(usize, usize),
    #[error("Invalid vector")]
    InvalidVector,
    #[error("Payload too large (max {0} bytes, got {1} bytes)")]
    PayloadTooLarge(usize, usize),
    #[error("Locked")]
    Locked,
    #[error("Already exists")]
    AlreadyExists,
    #[error("Read only")]
    ReadOnly,
    #[error("Internal mutex")]
    InternalLock,
}

pub type Result<T> = std::result::Result<T, EmveError>;

#[macro_export]
macro_rules! with_debug_log {
    ($ex:expr) => {
        $ex.inspect_err(|e| tracing::debug!("error at line {}: {:?}", line!(), e))
    };
}
