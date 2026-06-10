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
use std::ops::{Deref, DerefMut};

pub(super) trait FileLock {
    fn lock_file(&'_ mut self) -> crate::Result<FileLockGuard<'_>>;
    fn try_lock_file(&'_ mut self) -> crate::Result<FileLockGuard<'_>>;

    fn lock_file_shared(&'_ mut self) -> crate::Result<FileLockGuard<'_>>;
    fn try_lock_file_shared(&'_ mut self) -> crate::Result<FileLockGuard<'_>>;
}

pub(super) struct FileLockGuard<'a> {
    file: &'a mut File,
}

impl FileLock for File {
    fn lock_file(&'_ mut self) -> crate::Result<FileLockGuard<'_>> {
        self.lock()?;
        Ok(FileLockGuard { file: self })
    }

    fn try_lock_file(&'_ mut self) -> crate::Result<FileLockGuard<'_>> {
        match self.try_lock() {
            Ok(_) => Ok(FileLockGuard { file: self }),
            Err(e) => match e {
                TryLockError::Error(e) => Err(crate::EmveError::Io(e)),
                TryLockError::WouldBlock => Err(crate::EmveError::Locked),
            },
        }
    }

    fn lock_file_shared(&'_ mut self) -> crate::Result<FileLockGuard<'_>> {
        self.lock_shared()?;
        Ok(FileLockGuard { file: self })
    }

    fn try_lock_file_shared(&'_ mut self) -> crate::Result<FileLockGuard<'_>> {
        match self.try_lock_shared() {
            Ok(_) => Ok(FileLockGuard { file: self }),
            Err(e) => match e {
                TryLockError::Error(e) => Err(crate::EmveError::Io(e)),
                TryLockError::WouldBlock => Err(crate::EmveError::Locked),
            },
        }
    }
}

impl<'a> Deref for FileLockGuard<'a> {
    type Target = File;
    fn deref(&self) -> &Self::Target {
        self.file
    }
}

impl<'a> AsRef<File> for FileLockGuard<'a> {
    fn as_ref(&self) -> &File {
        self.file
    }
}

impl<'a> DerefMut for FileLockGuard<'a> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        self.file
    }
}

impl Drop for FileLockGuard<'_> {
    fn drop(&mut self) {
        self.file.unlock().unwrap();
    }
}
