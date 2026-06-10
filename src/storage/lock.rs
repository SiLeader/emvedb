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

use std::fs::{File, TryLockError};
use std::path::Path;

pub(super) struct HeldFileLock {
    file: File,
}

impl HeldFileLock {
    pub(super) fn exclusive(path: impl AsRef<Path>, wait: bool) -> crate::Result<Self> {
        let file = File::options().read(true).write(true).open(path)?;
        Self::lock(file, wait, false)
    }

    pub(super) fn shared(path: impl AsRef<Path>, wait: bool) -> crate::Result<Self> {
        let file = File::options().read(true).open(path)?;
        Self::lock(file, wait, true)
    }

    fn lock(file: File, wait: bool, shared: bool) -> crate::Result<Self> {
        if wait {
            if shared {
                file.lock_shared()?;
            } else {
                file.lock()?;
            }
        } else {
            let result = if shared {
                file.try_lock_shared()
            } else {
                file.try_lock()
            };
            match result {
                Ok(()) => {}
                Err(TryLockError::WouldBlock) => return Err(crate::EmveError::Locked),
                Err(TryLockError::Error(e)) => return Err(crate::EmveError::Io(e)),
            }
        }
        Ok(Self { file })
    }
}

impl Drop for HeldFileLock {
    fn drop(&mut self) {
        let _ = self.file.unlock();
    }
}
