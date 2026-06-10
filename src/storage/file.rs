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

use crate::element_type::ElementType;
use crate::format::{FrameRef, Header};
use crate::storage::Storage;
use crate::storage::lock::FileLock;
use crate::{EmveError, Metric, with_debug_log};
use std::fs::File;
use std::io::{Read, Seek, SeekFrom, Write};
use std::mem::replace;
use std::ops::DerefMut;
use std::sync::{Mutex, MutexGuard};

struct FileStorage {
    file_path: String,
    file: Mutex<File>,
}

impl FileStorage {
    pub fn open_readonly(path: &str) -> crate::Result<Self> {
        let file = File::options().read(true).open(path)?;
        let this = Self {
            file_path: path.to_string(),
            file: Mutex::new(file),
        };
        with_debug_log! { this.validate(false) }?;

        Ok(this)
    }

    pub fn open_readwrite(path: &str) -> crate::Result<Self> {
        let file = File::options()
            .read(true)
            .write(true)
            .append(true)
            .open(path)?;
        let this = Self {
            file_path: path.to_string(),
            file: Mutex::new(file),
        };
        with_debug_log! { this.validate(true) }?;
        Ok(this)
    }

    pub fn create_new(
        path: &str,
        metric: Metric,
        element_type: ElementType,
        dimension: u32,
    ) -> crate::Result<Self> {
        let file = File::options()
            .create_new(true)
            .write(true)
            .append(true)
            .read(true)
            .open(path)?;
        let header = Header::initial(metric, element_type, dimension);

        let mut this = Self {
            file_path: path.to_string(),
            file: Mutex::new(file),
        };
        this.write_header(&header)?;
        Ok(this)
    }

    fn get_file(&self) -> crate::Result<MutexGuard<'_, File>> {
        with_debug_log! { self.file.lock() }.map_err(|_| EmveError::InternalLock)
    }

    fn validate(&self, truncate: bool) -> crate::Result<()> {
        let mut file = self.get_file()?;
        let mut file_locked = with_debug_log! { file.try_lock_file() }?;
        file_locked.seek(SeekFrom::Start(0))?;
        if let Err(e) = validate(file_locked.deref_mut()) {
            match e {
                ValidateFail::Error(e) => Err(e),
                ValidateFail::TruncateRequired(len) => {
                    if truncate {
                        with_debug_log! { file_locked.set_len(len) }?;
                    }
                    Ok(())
                }
            }
        } else {
            Ok(())
        }
    }
}

impl Storage for FileStorage {
    fn append(&mut self, bytes: &[u8]) -> crate::Result<u64> {
        let mut file = self.get_file()?;
        let mut file_locked = with_debug_log! { file.try_lock_file() }?;

        let offset = with_debug_log! { file_locked.seek(SeekFrom::End(0)) }?;
        with_debug_log! { file_locked.write_all(bytes) }?;
        Ok(offset)
    }

    fn read_at(&self, offset: u64, len: usize) -> crate::Result<Vec<u8>> {
        let mut file = self.get_file()?;
        let mut file_locked = with_debug_log! { file.try_lock_file_shared() }?;

        with_debug_log! { file_locked.seek(SeekFrom::Start(offset)) }?;
        let mut buf = vec![0u8; len];
        with_debug_log! { file_locked.read_exact(&mut buf) }?;
        Ok(buf)
    }

    fn len(&self) -> crate::Result<u64> {
        let mut file = self.get_file()?;
        let mut file_locked = with_debug_log! { file.try_lock_file_shared() }?;

        let offset = with_debug_log! { file_locked.seek(SeekFrom::End(0)) }?;
        Ok(offset)
    }

    fn sync(&mut self) -> crate::Result<()> {
        let mut file = self.get_file()?;
        let file_locked = with_debug_log! { file.try_lock_file() }?;

        with_debug_log! { file_locked.sync_all() }?;
        Ok(())
    }

    fn truncate(&mut self, len: u64) -> crate::Result<()> {
        let mut file = self.get_file()?;
        let file_locked = with_debug_log! { file.try_lock_file() }?;

        with_debug_log! { file_locked.set_len(len) }?;
        Ok(())
    }

    fn write_header(&mut self, header: &Header) -> crate::Result<()> {
        let mut file = self.get_file()?;
        let mut file_locked = with_debug_log! { file.try_lock_file() }?;

        with_debug_log! { file_locked.seek(SeekFrom::Start(0)) }?;
        with_debug_log! { file_locked.write_all(&header.encode()) }?;
        Ok(())
    }

    fn recreate(&mut self, new_data: &[u8], suffix: &str) -> crate::Result<()> {
        let mut file = with_debug_log! { self.get_file() }?;

        let recreated_file_path = format!("{}.{suffix}", self.file_path);
        let mut recreate_file = with_debug_log! { File::create_new(&recreated_file_path) }?;
        with_debug_log! { recreate_file.write_all(new_data) }?;
        with_debug_log! { recreate_file.sync_all() }?;

        let _old = replace(&mut *file, recreate_file);

        let old_renamed_path = format!("{}.{suffix}.old", self.file_path);
        with_debug_log! { std::fs::rename(&self.file_path, &old_renamed_path) }?;
        with_debug_log! { std::fs::rename(&recreated_file_path, &self.file_path) }?;

        Ok(())
    }
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

fn validate(source: &mut impl Read) -> Result<(Header, Vec<FrameRef>), ValidateFail> {
    let mut current_size = 0u64;
    let header = {
        let mut header_bytes = [0u8; 64];
        source
            .read_exact(&mut header_bytes)
            .inspect_err(|e| tracing::error!("{:?}", e))?;
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
                    e => with_debug_log! { Err(ValidateFail::Error(e.into())) }?,
                };
            }
        }
    }

    Ok((header, frames))
}

fn read_length_or_none(
    source: &mut impl Read,
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
