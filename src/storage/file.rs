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
use crate::format::Header;
use crate::storage::Storage;
use crate::{EmveError, Metric};
use std::fs::File;
use std::io::{Read, Seek, SeekFrom, Write};
use std::mem::replace;
use std::sync::{Mutex, MutexGuard};

struct FileStorage {
    file_path: String,
    file: Mutex<File>,
}

impl FileStorage {
    pub fn open_readonly(path: &str) -> crate::Result<Self> {
        let mut file = File::options().read(true).open(path)?;
        let mut header_buf = [0u8; 64];
        file.read_exact(&mut header_buf)?;
        Ok(Self {
            file_path: path.to_string(),
            file: Mutex::new(file),
        })
    }

    pub fn open_readwrite(path: &str) -> crate::Result<Self> {
        let file = File::options().read(true).write(true).open(path)?;
        Ok(Self {
            file_path: path.to_string(),
            file: Mutex::new(file),
        })
    }

    pub fn create_new(
        path: &str,
        metric: Metric,
        element_type: ElementType,
        dimension: u32,
    ) -> crate::Result<Self> {
        let file = File::options().create_new(true).read(true).open(path)?;
        let header = Header::initial(metric, element_type, dimension);

        let mut this = Self {
            file_path: path.to_string(),
            file: Mutex::new(file),
        };
        this.write_header(&header)?;
        Ok(this)
    }

    fn get_file(&self) -> crate::Result<MutexGuard<'_, File>> {
        self.file.lock().map_err(|_| EmveError::InternalLock)
    }
}

impl Storage for FileStorage {
    fn append(&mut self, bytes: &[u8]) -> crate::Result<u64> {
        let mut file = self.get_file()?;
        let offset = file.seek(SeekFrom::End(0))?;
        file.write_all(bytes)?;
        Ok(offset)
    }

    fn read_at(&self, offset: u64, len: usize) -> crate::Result<Vec<u8>> {
        let mut file = self.get_file()?;
        file.seek(SeekFrom::Start(offset))?;
        let mut buf = vec![0u8; len];
        file.read_exact(&mut buf)?;
        Ok(buf)
    }

    fn len(&self) -> crate::Result<u64> {
        let mut file = self.get_file()?;
        let offset = file.seek(SeekFrom::End(0))?;
        Ok(offset)
    }

    fn sync(&mut self) -> crate::Result<()> {
        let file = self.get_file()?;
        file.sync_all()?;
        Ok(())
    }

    fn truncate(&mut self, len: u64) -> crate::Result<()> {
        let file = self.get_file()?;
        file.set_len(len)?;
        Ok(())
    }

    fn write_header(&mut self, header: &Header) -> crate::Result<()> {
        let mut file = self.get_file()?;
        file.seek(SeekFrom::Start(0))?;
        file.write_all(&header.encode())?;
        Ok(())
    }

    fn recreate(&mut self, new_data: &[u8], suffix: &str) -> crate::Result<()> {
        let mut file = self.get_file()?;
        let recreated_file_path = format!("{}.{suffix}", self.file_path);
        let mut recreate_file = File::create_new(&recreated_file_path)?;
        recreate_file.write_all(new_data)?;
        recreate_file.sync_all()?;

        let _old = replace(&mut *file, recreate_file);

        let old_renamed_path = format!("{}.{suffix}.old", self.file_path);
        std::fs::rename(&self.file_path, &old_renamed_path)?;
        std::fs::rename(&recreated_file_path, &self.file_path)?;

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_open_and_write_multiple_times() {
        let dir = tempfile::tempdir().expect("create temp file");
        let path = dir.path().join("test_open_and_write_multiple_times.evdb");
        let file_name = path.to_str().unwrap();
        {
            let mut storage =
                FileStorage::create_new(file_name, Metric::Cosine, ElementType::F32, 3)
                    .expect("create file");
            storage.append(&[1, 2, 3]).expect("write");
            storage.append(&[4, 5, 6]).expect("write");
            storage.append(&[7, 8, 9]).expect("write");
        }
        {
            let storage = FileStorage::open_readwrite(file_name).expect("open file");
            assert_eq!(storage.read_at(0, 3).expect("read"), vec![1, 2, 3]);
            assert_eq!(storage.read_at(3, 3).expect("read"), vec![4, 5, 6]);
            assert_eq!(storage.read_at(6, 3).expect("read"), vec![7, 8, 9]);
        }
    }
}
