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

use crate::EmveError;
use crate::format::{FrameRef, Header};

mod file;
mod lock;
mod memory;

trait Storage {
    fn append(&mut self, bytes: &[u8]) -> crate::Result<u64>;
    fn read_at(&self, offset: u64, len: usize) -> crate::Result<Vec<u8>>;
    fn len(&self) -> crate::Result<u64>;
    fn sync(&mut self) -> crate::Result<()>;
    fn truncate(&mut self, len: u64) -> crate::Result<()>;
    fn write_header(&mut self, header: &Header) -> crate::Result<()>;
    fn recreate(&mut self, new_data: &[u8], suffix: &str) -> crate::Result<()>;
}

#[derive(Debug, thiserror::Error)]
enum ValidateFail {
    #[error("Error unrecoverable")]
    Error(#[from] EmveError),
    #[error("Truncate required")]
    TruncateRequired(u64),
}

impl From<std::io::Error> for ValidateFail {
    fn from(value: std::io::Error) -> Self {
        ValidateFail::Error(value.into())
    }
}

fn validate(source: &mut impl std::io::Read) -> Result<(Header, Vec<FrameRef>), ValidateFail> {
    let mut current_size = 0u64;
    let header = {
        let mut header_bytes = [0u8; 64];
        source.read_exact(&mut header_bytes)?;
        current_size += header_bytes.len() as u64;
        Header::decode(&header_bytes)?
    };

    let mut frames = Vec::new();
    loop {
        let Some(length) = read_length_or_none(source, current_size)? else {
            break;
        };

        match FrameRef::decode_with_length(source, header.dimension, length as usize) {
            Ok(frame) => {
                current_size += 4 + length as u64;
                frames.push(frame);
            }
            Err(e) => {
                return match e {
                    EmveError::Io(e) if e.kind() == std::io::ErrorKind::UnexpectedEof => {
                        Err(ValidateFail::TruncateRequired(current_size))
                    }
                    e => Err(ValidateFail::Error(e.into())),
                };
            }
        }
    }

    Ok((header, frames))
}

fn read_length_or_none(
    source: &mut impl std::io::Read,
    current_size: u64,
) -> Result<Option<u32>, ValidateFail> {
    let mut length_bytes = [0u8; 4];

    let mut offset = 0;
    loop {
        let read_len = source.read(&mut length_bytes[offset..])?;
        if read_len == 0 {
            return if offset == 0 {
                // EOF
                Ok(None)
            } else {
                Err(ValidateFail::TruncateRequired(current_size))
            };
        }
        if offset == length_bytes.len() {
            return Ok(Some(u32::from_le_bytes(length_bytes)));
        }
        offset += read_len;
    }
}

#[cfg(test)]
mod tests {
    use crate::storage::Storage;

    fn generic_test<S: Storage>() {}
}
