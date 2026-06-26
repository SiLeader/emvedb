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
use crate::format::{FrameRef, HEADER_SIZE, Header};
use crate::storage::Storage;
use crate::storage::lock::HeldFileLock;
use crate::{EmveError, Metric, with_debug_log};
use std::fs::File;
use std::io::{Read, Seek, SeekFrom, Write};
use std::mem::replace;
use std::sync::{Mutex, MutexGuard};

pub(crate) struct FileStorage {
    file_path: String,
    file: Mutex<File>,
    logical_len: Mutex<u64>,
    _lock: HeldFileLock,
}

impl FileStorage {
    pub fn open_readonly(path: &str, lock_wait: bool) -> crate::Result<Self> {
        let lock = HeldFileLock::shared(path, lock_wait)?;
        cleanup_recreate_artifacts(path, "compact")?;
        let file = File::options().read(true).open(path)?;
        let this = Self {
            file_path: path.to_string(),
            file: Mutex::new(file),
            logical_len: Mutex::new(0),
            _lock: lock,
        };
        let logical_len = with_debug_log! { this.validate(false) }?;
        *this
            .logical_len
            .lock()
            .map_err(|_| EmveError::InternalLock)? = logical_len;

        Ok(this)
    }

    pub fn open_readwrite(path: &str, lock_wait: bool) -> crate::Result<Self> {
        let lock = HeldFileLock::exclusive(path, lock_wait)?;
        cleanup_recreate_artifacts(path, "compact")?;
        let file = File::options().read(true).append(true).open(path)?;
        let this = Self {
            file_path: path.to_string(),
            file: Mutex::new(file),
            logical_len: Mutex::new(0),
            _lock: lock,
        };
        let logical_len = with_debug_log! { this.validate(true) }?;
        *this
            .logical_len
            .lock()
            .map_err(|_| EmveError::InternalLock)? = logical_len;
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
            .append(true)
            .read(true)
            .open(path)?;
        let lock = HeldFileLock::exclusive(path, false)?;
        let header = Header::initial(metric, element_type, dimension);

        let mut this = Self {
            file_path: path.to_string(),
            file: Mutex::new(file),
            logical_len: Mutex::new(0),
            _lock: lock,
        };
        this.write_header(&header)?;
        Ok(this)
    }

    pub fn create_new_from_options(
        path: &str,
        options: &crate::CreateOptions,
    ) -> crate::Result<Self> {
        Self::create_new(path, options.metric, ElementType::F32, options.dimension)
    }

    fn get_file(&self) -> crate::Result<MutexGuard<'_, File>> {
        with_debug_log! { self.file.lock() }.map_err(|_| EmveError::InternalLock)
    }

    fn validate(&self, truncate: bool) -> crate::Result<u64> {
        let mut file = self.get_file()?;
        file.seek(SeekFrom::Start(0))?;
        if let Err(e) = validate(&mut *file) {
            match e {
                ValidateFail::Error(e) => Err(e),
                ValidateFail::TruncateRequired(len) => {
                    if truncate {
                        with_debug_log! { file.set_len(len) }?;
                    }
                    Ok(len)
                }
            }
        } else {
            Ok(with_debug_log! { file.seek(SeekFrom::End(0)) }?)
        }
    }
}

impl Storage for FileStorage {
    fn append(&mut self, bytes: &[u8]) -> crate::Result<u64> {
        let mut file = self.get_file()?;

        let offset = with_debug_log! { file.seek(SeekFrom::End(0)) }?;
        with_debug_log! { file.write_all(bytes) }?;
        *self
            .logical_len
            .lock()
            .map_err(|_| EmveError::InternalLock)? = offset + bytes.len() as u64;
        Ok(offset)
    }

    fn read_at(&self, offset: u64, len: usize) -> crate::Result<Vec<u8>> {
        let logical_len = *self
            .logical_len
            .lock()
            .map_err(|_| EmveError::InternalLock)?;
        if offset + len as u64 > logical_len {
            return Err(std::io::Error::from(std::io::ErrorKind::UnexpectedEof).into());
        }
        let mut file = self.get_file()?;

        with_debug_log! { file.seek(SeekFrom::Start(offset)) }?;
        let mut buf = vec![0u8; len];
        with_debug_log! { file.read_exact(&mut buf) }?;
        Ok(buf)
    }

    fn len(&self) -> crate::Result<u64> {
        self.logical_len
            .lock()
            .map(|len| *len)
            .map_err(|_| EmveError::InternalLock)
    }

    fn sync(&mut self) -> crate::Result<()> {
        let file = self.get_file()?;

        with_debug_log! { file.sync_all() }?;
        Ok(())
    }

    fn write_header(&mut self, header: &Header) -> crate::Result<()> {
        let mut file = self.get_file()?;

        with_debug_log! { file.seek(SeekFrom::Start(0)) }?;
        with_debug_log! { file.write_all(&header.encode()) }?;
        let mut logical_len = self
            .logical_len
            .lock()
            .map_err(|_| EmveError::InternalLock)?;
        *logical_len = (*logical_len).max(HEADER_SIZE as u64);
        Ok(())
    }

    fn recreate(&mut self, new_data: &[u8], suffix: &str) -> crate::Result<()> {
        let mut file = with_debug_log! { self.get_file() }?;

        let recreated_file_path = format!("{}.{suffix}", self.file_path);
        let old_renamed_path = format!("{}.{suffix}.old", self.file_path);
        remove_file_if_exists(&recreated_file_path)?;
        remove_file_if_exists(&old_renamed_path)?;
        let mut recreate_file = with_debug_log! {
            File::options()
                .create_new(true)
                .read(true)
                .append(true)
                .open(&recreated_file_path)
        }?;
        with_debug_log! { recreate_file.write_all(new_data) }?;
        with_debug_log! { recreate_file.sync_all() }?;

        let old = replace(&mut *file, recreate_file);

        with_debug_log! { std::fs::rename(&self.file_path, &old_renamed_path) }?;
        with_debug_log! { std::fs::rename(&recreated_file_path, &self.file_path) }?;
        drop(old);
        remove_file_if_exists(&old_renamed_path)?;
        *self
            .logical_len
            .lock()
            .map_err(|_| EmveError::InternalLock)? = new_data.len() as u64;

        Ok(())
    }
}

fn cleanup_recreate_artifacts(path: &str, suffix: &str) -> crate::Result<()> {
    remove_file_if_exists(format!("{path}.{suffix}"))?;
    remove_file_if_exists(format!("{path}.{suffix}.old"))
}

fn remove_file_if_exists(path: impl AsRef<std::path::Path>) -> crate::Result<()> {
    match std::fs::remove_file(path) {
        Ok(()) => Ok(()),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(e) => Err(e.into()),
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

fn validate(source: &mut (impl Read + Seek)) -> Result<(Header, Vec<FrameRef>), ValidateFail> {
    let file_len = source.seek(SeekFrom::End(0))?;
    source.seek(SeekFrom::Start(0))?;

    if file_len < HEADER_SIZE as u64 {
        return Err(ValidateFail::Error(EmveError::Corrupt));
    }

    let mut current_size = 0u64;
    let header = {
        let mut header_bytes = [0u8; HEADER_SIZE];
        source
            .read_exact(&mut header_bytes)
            .inspect_err(|e| tracing::error!("{:?}", e))?;
        current_size += header_bytes.len() as u64;
        Header::decode(&header_bytes)?
    };

    let mut frames = Vec::new();
    while current_size < file_len {
        let frame_start = current_size;
        let Some(length) = read_length_or_none(source, current_size)? else {
            break;
        };
        current_size += 4;

        if length < 4 {
            return Err(ValidateFail::Error(EmveError::Corrupt));
        }

        let frame_end = current_size + length as u64;
        if frame_end > file_len {
            return Err(ValidateFail::TruncateRequired(frame_start));
        }

        match FrameRef::decode_with_length(source, header.dimension, length as usize) {
            Ok(frame) => {
                current_size = frame_end;
                frames.push(frame);
            }
            Err(e) => {
                if is_truncated_frame_error(&e) || is_crc_error(&e) && frame_end == file_len {
                    return Err(ValidateFail::TruncateRequired(frame_start));
                }
                return Err(ValidateFail::Error(EmveError::Corrupt));
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
        offset += read_len;
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
    }
}

fn is_truncated_frame_error(error: &EmveError) -> bool {
    matches!(error, EmveError::Io(e) if e.kind() == std::io::ErrorKind::UnexpectedEof)
}

fn is_crc_error(error: &EmveError) -> bool {
    matches!(
        error,
        EmveError::InvalidFrame(crate::format::FrameError::CrcMismatch { .. })
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::format::Frame;
    use crate::storage::storage::EmvedbStorage;
    use std::fs::OpenOptions;

    fn path_string(path: &std::path::Path) -> String {
        path.to_str().unwrap().to_string()
    }

    fn put(id: u64) -> Frame<'static> {
        Frame::Put {
            id,
            vector: &[1.0, 2.0, 3.0],
            payload: b"payload",
        }
    }

    #[test]
    fn create_reopen_and_scan_frames() {
        let dir = tempfile::tempdir().unwrap();
        let path = path_string(&dir.path().join("db.evdb"));

        {
            let file = FileStorage::create_new(&path, Metric::Cosine, ElementType::F32, 3).unwrap();
            let mut storage = EmvedbStorage::new(file);
            storage.append_frame(put(1)).unwrap();
            storage.append_frame(Frame::Delete { id: 2 }).unwrap();
            storage.append_frame(put(3)).unwrap();
        }

        let file = FileStorage::open_readwrite(&path, false).unwrap();
        let storage = EmvedbStorage::new(file);
        let frames = storage.read_all_frames().unwrap();
        assert_eq!(frames.len(), 3);
    }

    #[test]
    fn empty_file_after_header_scans_as_empty() {
        let dir = tempfile::tempdir().unwrap();
        let path = path_string(&dir.path().join("empty.evdb"));

        let file = FileStorage::create_new(&path, Metric::Cosine, ElementType::F32, 3).unwrap();
        let storage = EmvedbStorage::new(file);

        assert!(storage.read_all_frames().unwrap().is_empty());
    }

    #[test]
    fn open_readwrite_truncates_torn_tail() {
        let dir = tempfile::tempdir().unwrap();
        let path = path_string(&dir.path().join("torn.evdb"));
        let valid_len;

        {
            let file = FileStorage::create_new(&path, Metric::Cosine, ElementType::F32, 3).unwrap();
            let mut storage = EmvedbStorage::new(file);
            storage.append_frame(put(1)).unwrap();
            valid_len = storage.into_inner().len().unwrap();
        }
        {
            let mut raw = OpenOptions::new().append(true).open(&path).unwrap();
            let encoded = put(2).encode();
            raw.write_all(&encoded[..encoded.len() - 3]).unwrap();
        }

        let file = FileStorage::open_readwrite(&path, false).unwrap();
        assert_eq!(file.len().unwrap(), valid_len);
        let storage = EmvedbStorage::new(file);
        assert_eq!(storage.read_all_frames().unwrap().len(), 1);
    }

    #[test]
    fn open_readwrite_truncates_last_crc_mismatch() {
        let dir = tempfile::tempdir().unwrap();
        let path = path_string(&dir.path().join("crc-tail.evdb"));
        let valid_len;

        {
            let file = FileStorage::create_new(&path, Metric::Cosine, ElementType::F32, 3).unwrap();
            let mut storage = EmvedbStorage::new(file);
            storage.append_frame(put(1)).unwrap();
            valid_len = storage.into_inner().len().unwrap();
        }
        {
            let mut raw = OpenOptions::new().append(true).open(&path).unwrap();
            let mut encoded = put(2).encode();
            *encoded.last_mut().unwrap() ^= 0x01;
            raw.write_all(&encoded).unwrap();
        }

        let file = FileStorage::open_readwrite(&path, false).unwrap();
        assert_eq!(file.len().unwrap(), valid_len);
        let storage = EmvedbStorage::new(file);
        assert_eq!(storage.read_all_frames().unwrap().len(), 1);
    }

    #[test]
    fn middle_crc_mismatch_is_corrupt() {
        let dir = tempfile::tempdir().unwrap();
        let path = path_string(&dir.path().join("middle-crc.evdb"));

        {
            let file = FileStorage::create_new(&path, Metric::Cosine, ElementType::F32, 3).unwrap();
            let mut storage = EmvedbStorage::new(file);
            storage.append_frame(put(1)).unwrap();
            storage.append_frame(put(2)).unwrap();
        }
        {
            let mut raw = OpenOptions::new()
                .read(true)
                .write(true)
                .open(&path)
                .unwrap();
            raw.seek(SeekFrom::Start(HEADER_SIZE as u64 + 4)).unwrap();
            raw.write_all(&[0xff]).unwrap();
        }

        assert!(matches!(
            FileStorage::open_readwrite(&path, false),
            Err(EmveError::Corrupt)
        ));
    }

    #[test]
    fn locks_block_conflicting_opens_and_allow_shared_readers() {
        let dir = tempfile::tempdir().unwrap();
        let path = path_string(&dir.path().join("lock.evdb"));

        let writer = FileStorage::create_new(&path, Metric::Cosine, ElementType::F32, 3).unwrap();
        assert!(matches!(
            FileStorage::open_readwrite(&path, false),
            Err(EmveError::Locked)
        ));
        assert!(matches!(
            FileStorage::open_readonly(&path, false),
            Err(EmveError::Locked)
        ));
        drop(writer);

        let reader = FileStorage::open_readonly(&path, false).unwrap();
        let second_reader = FileStorage::open_readonly(&path, false).unwrap();
        assert!(matches!(
            FileStorage::open_readwrite(&path, false),
            Err(EmveError::Locked)
        ));
        drop(second_reader);
        drop(reader);
    }
}
