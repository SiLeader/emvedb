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

mod frame;
mod header;

pub use frame::*;
pub use header::*;

#[derive(Debug, thiserror::Error)]
pub enum FrameError {
    #[error("Invalid frame type: {0}")]
    FrameType(u8),
    #[error("Invalid byte range: {0}..{1}")]
    ByteRange(usize, usize),
    #[error("CRC mismatch: expected {expected:x}, actual {actual:x}")]
    CrcMismatch { expected: u32, actual: u32 },
}

#[derive(Debug, thiserror::Error)]
pub enum HeaderError {
    #[error("Invalid header length: {0}")]
    LengthTooShort(usize),
    #[error("Invalid metric: {0}")]
    Metric(u8),
    #[error("Invalid element type: {0}")]
    ElementType(u8),
}
