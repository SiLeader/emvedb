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

use crate::format::FrameRef;
use std::collections::HashMap;

pub(crate) struct Entry {
    pub slot: u32,
    pub payload_offset: u64,
    pub payload_len: u32,
    pub inv_norm: f32,
}

pub(crate) struct InMemoryIndex {
    dimension: u32,
    mapping: HashMap<u64, Entry>,
    arena: Vec<f32>,
    free_slots: Vec<u32>,
    live: usize,
}

impl InMemoryIndex {
    pub fn new_empty(dimension: u32) -> Self {
        Self {
            dimension,
            mapping: HashMap::new(),
            arena: vec![],
            free_slots: vec![],
            live: 0,
        }
    }

    pub fn build_from_frames(dimension: u32, frames: &[FrameRef]) -> Self {
        let mut this = Self::new_empty(dimension);
        for frame in frames {
            match frame {
                FrameRef::Put {
                    id,
                    vector,
                    payload_range,
                } => {
                    this.put(
                        *id,
                        vector,
                        payload_range.start,
                        (payload_range.end - payload_range.start) as u32,
                    );
                }
                FrameRef::Delete { id } => {
                    this.delete(*id);
                }
            }
        }
        this
    }

    pub fn put(&mut self, id: u64, vector: &[f32], payload_offset: u64, payload_len: u32) {
        let free_slot = self.free_slots.pop();
        if let Some(slot) = free_slot {
            let arena_range =
                (slot * self.dimension) as usize..((slot + 1) * self.dimension) as usize;
            self.arena[arena_range].copy_from_slice(vector);
        } else {
            self.arena.extend_from_slice(vector);
        }
        let prev = self.mapping.insert(
            id,
            Entry {
                slot: free_slot.unwrap_or(self.live as u32),
                payload_offset,
                payload_len,
                inv_norm: 1.0 / vector.iter().map(|x| x * x).sum::<f32>().sqrt(),
            },
        );
        if let Some(prev) = prev {
            self.free_slots.push(prev.slot);
        } else {
            self.live += 1;
        }
    }

    pub fn delete(&mut self, id: u64) -> bool {
        if let Some(entry) = self.mapping.remove(&id) {
            self.free_slots.push(entry.slot);
            self.live -= 1;
            true
        } else {
            false
        }
    }

    pub fn get_entry(&self, id: u64) -> Option<&Entry> {
        self.mapping.get(&id)
    }

    pub fn contains(&self, id: u64) -> bool {
        self.mapping.contains_key(&id)
    }

    pub fn vector_of(&self, slot: u32) -> &[f32] {
        let arena_range = (slot * self.dimension) as usize..((slot + 1) * self.dimension) as usize;
        &self.arena[arena_range]
    }

    pub fn len(&self) -> usize {
        self.mapping.len()
    }

    pub fn is_empty(&self) -> bool {
        self.mapping.is_empty()
    }

    pub fn iter_live(&self) -> impl Iterator<Item = (u64, &Entry, &[f32])> {
        self.mapping
            .iter()
            .map(|(&id, entry)| (id, entry, self.vector_of(entry.slot)))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_repeat_same_id_put() {
        let mut index = InMemoryIndex::new_empty(3);
        index.put(1, &[0.1, 0.2, 0.3], 0, 5);
        index.put(1, &[0.4, 0.5, 0.6], 5, 5);

        assert_eq!(index.len(), 1);
        assert!(index.contains(1));
        let entry = index.get_entry(1).unwrap();
        assert_eq!(entry.payload_offset, 5);
        assert_eq!(entry.payload_len, 5);
        assert_eq!(index.vector_of(entry.slot), &[0.4, 0.5, 0.6]);
    }

    #[test]
    fn test_delete_and_contains() {
        let mut index = InMemoryIndex::new_empty(3);
        index.put(1, &[0.1, 0.2, 0.3], 0, 5);
        index.put(2, &[0.4, 0.5, 0.6], 5, 5);
        index.delete(1);

        assert_eq!(index.len(), 1);
        assert!(!index.contains(1));
        assert!(index.contains(2));
        let entry = index.get_entry(2).unwrap();
        assert_eq!(entry.payload_offset, 5);
        assert_eq!(entry.payload_len, 5);
    }

    #[test]
    fn test_build_from_frames() {
        let mut index = InMemoryIndex::new_empty(3);
        index.put(1, &[0.1, 0.2, 0.3], 0, 5);
        index.put(1, &[0.4, 0.5, 0.6], 5, 5);
        index.delete(1);
        index.put(2, &[0.7, 0.8, 0.9], 10, 5);

        assert_eq!(index.len(), 1);
        assert!(!index.contains(1));
        assert!(index.contains(2));
        let entry = index.get_entry(2).unwrap();
        assert_eq!(entry.payload_offset, 10);
        assert_eq!(entry.payload_len, 5);
        assert_eq!(index.vector_of(entry.slot), &[0.7, 0.8, 0.9]);
    }

    #[test]
    fn test_vector_position() {
        let mut index = InMemoryIndex::new_empty(3);
        index.put(1, &[0.1, 0.2, 0.3], 0, 5);
        index.put(2, &[0.4, 0.5, 0.6], 5, 5);
        index.put(3, &[0.7, 0.8, 0.9], 10, 5);

        assert_eq!(index.vector_of(0), &[0.1, 0.2, 0.3]);
        assert_eq!(index.vector_of(1), &[0.4, 0.5, 0.6]);
        assert_eq!(index.vector_of(2), &[0.7, 0.8, 0.9]);
    }
}
