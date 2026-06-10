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

use crate::storage::Storage;

pub struct MemoryStorage {
    data: Vec<u8>,
}

impl MemoryStorage {
    pub fn new(data: Vec<u8>) -> Self {
        Self { data }
    }
}

impl Storage for MemoryStorage {
    fn append(&mut self, bytes: &[u8]) -> crate::Result<u64> {
        let offset = self.data.len() as u64;
        self.data.extend_from_slice(bytes);
        Ok(offset)
    }

    fn read_at(&self, offset: u64, len: usize) -> crate::Result<Vec<u8>> {
        let offset = offset as usize;
        if offset + len > self.data.len() {
            return Err(crate::EmveError::Io(std::io::ErrorKind::InvalidData.into()));
        }
        Ok(self.data[offset..offset + len].to_vec())
    }

    fn len(&self) -> crate::Result<u64> {
        Ok(self.data.len() as u64)
    }

    fn sync(&mut self) -> crate::Result<()> {
        Ok(())
    }

    fn truncate(&mut self, len: u64) -> crate::Result<()> {
        self.data.truncate(len as usize);
        Ok(())
    }

    fn write_header(&mut self, header: &crate::format::Header) -> crate::Result<()> {
        let header_bytes = header.encode();
        self.data.splice(0..header_bytes.len(), header_bytes);
        Ok(())
    }

    fn recreate(&mut self, new_data: &[u8], _suffix: &str) -> crate::Result<()> {
        self.data = new_data.to_vec();
        Ok(())
    }
}
