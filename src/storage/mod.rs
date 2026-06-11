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

use crate::format::Header;

pub(crate) mod file;
mod lock;
pub(crate) mod memory;
pub(crate) mod storage;

pub(crate) trait Storage {
    fn append(&mut self, bytes: &[u8]) -> crate::Result<u64>;
    fn read_at(&self, offset: u64, len: usize) -> crate::Result<Vec<u8>>;
    fn len(&self) -> crate::Result<u64>;
    fn sync(&mut self) -> crate::Result<()>;
    fn truncate(&mut self, len: u64) -> crate::Result<()>;
    fn write_header(&mut self, header: &Header) -> crate::Result<()>;
    fn recreate(&mut self, new_data: &[u8], suffix: &str) -> crate::Result<()>;
}
impl Storage for Box<dyn Storage> {
    fn append(&mut self, bytes: &[u8]) -> crate::Result<u64> {
        (**self).append(bytes)
    }
    fn read_at(&self, offset: u64, len: usize) -> crate::Result<Vec<u8>> {
        (**self).read_at(offset, len)
    }
    fn len(&self) -> crate::Result<u64> {
        (**self).len()
    }
    fn sync(&mut self) -> crate::Result<()> {
        (**self).sync()
    }
    fn truncate(&mut self, len: u64) -> crate::Result<()> {
        (**self).truncate(len)
    }
    fn write_header(&mut self, header: &Header) -> crate::Result<()> {
        (**self).write_header(header)
    }
    fn recreate(&mut self, new_data: &[u8], suffix: &str) -> crate::Result<()> {
        (**self).recreate(new_data, suffix)
    }
}
