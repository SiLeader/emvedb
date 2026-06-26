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
use crate::metric::compute_inv_norm;
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
        assert!(dimension > 0, "dimension must be greater than zero");
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

    pub fn dimension(&self) -> u32 {
        self.dimension
    }

    pub fn put(&mut self, id: u64, vector: &[f32], payload_offset: u64, payload_len: u32) {
        assert_eq!(
            vector.len(),
            self.dimension as usize,
            "vector length must match index dimension"
        );
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
                inv_norm: compute_inv_norm(vector),
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
    use proptest::prelude::*;
    use std::collections::{HashMap, HashSet};

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
        let deleted_slot = index.get_entry(1).unwrap().slot;
        index.delete(1);

        assert_eq!(index.len(), 1);
        assert!(!index.contains(1));
        assert!(index.contains(2));
        let entry = index.get_entry(2).unwrap();
        assert_eq!(entry.payload_offset, 5);
        assert_eq!(entry.payload_len, 5);

        index.put(3, &[0.7, 0.8, 0.9], 10, 5);
        let entry = index.get_entry(3).unwrap();
        assert_eq!(entry.slot, deleted_slot);
        assert_eq!(index.vector_of(entry.slot), &[0.7, 0.8, 0.9]);
    }

    #[test]
    fn test_build_from_frames() {
        let frames = vec![
            FrameRef::Put {
                id: 1,
                vector: vec![0.1, 0.2, 0.3],
                payload_range: 0..5,
            },
            FrameRef::Put {
                id: 1,
                vector: vec![0.4, 0.5, 0.6],
                payload_range: 5..10,
            },
            FrameRef::Delete { id: 1 },
            FrameRef::Put {
                id: 2,
                vector: vec![0.7, 0.8, 0.9],
                payload_range: 10..15,
            },
        ];
        let index = InMemoryIndex::build_from_frames(3, &frames);

        assert_eq!(index.len(), 1);
        assert!(!index.contains(1));
        assert!(index.contains(2));
        let entry = index.get_entry(2).unwrap();
        assert_eq!(entry.payload_offset, 10);
        assert_eq!(entry.payload_len, 5);
        assert_eq!(index.vector_of(entry.slot), &[0.7, 0.8, 0.9]);
    }

    #[test]
    fn test_zero_vector_inv_norm_is_zero() {
        let mut index = InMemoryIndex::new_empty(3);
        index.put(1, &[0.0, 0.0, 0.0], 0, 0);

        assert_eq!(index.get_entry(1).unwrap().inv_norm, 0.0);
    }

    #[test]
    #[should_panic(expected = "vector length must match index dimension")]
    fn test_rejects_dimension_mismatch() {
        let mut index = InMemoryIndex::new_empty(3);
        index.put(1, &[1.0, 2.0], 0, 0);
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

    proptest! {
        #[test]
        fn upsert_delete_preserve_index_invariants(
            ops in prop::collection::vec((0u64..8, any::<bool>(), -1000i32..1000), 1..200)
        ) {
            let mut index = InMemoryIndex::new_empty(2);
            let mut expected = HashMap::<u64, ([f32; 2], u64, u32)>::new();

            for (id, should_put, seed) in ops {
                if should_put {
                    let vector = [id as f32, seed as f32];
                    let payload_offset = id * 100 + seed.unsigned_abs() as u64;
                    let payload_len = (seed.unsigned_abs() % 17) + 1;
                    index.put(id, &vector, payload_offset, payload_len);
                    expected.insert(id, (vector, payload_offset, payload_len));
                } else {
                    prop_assert_eq!(index.delete(id), expected.remove(&id).is_some());
                }

                assert_index_matches(&index, &expected)?;
            }
        }
    }

    fn assert_index_matches(
        index: &InMemoryIndex,
        expected: &HashMap<u64, ([f32; 2], u64, u32)>,
    ) -> Result<(), TestCaseError> {
        let total_slots = index.arena.len() / index.dimension as usize;
        prop_assert_eq!(index.arena.len() % index.dimension as usize, 0);
        prop_assert_eq!(index.live, expected.len());
        prop_assert_eq!(index.len(), expected.len());
        prop_assert_eq!(index.mapping.len(), expected.len());
        prop_assert_eq!(index.live + index.free_slots.len(), total_slots);

        let mut used_slots = HashSet::new();
        for (&id, entry) in &index.mapping {
            let (vector, payload_offset, payload_len) = expected.get(&id).unwrap();
            prop_assert!(used_slots.insert(entry.slot));
            prop_assert_eq!(entry.payload_offset, *payload_offset);
            prop_assert_eq!(entry.payload_len, *payload_len);
            prop_assert_eq!(index.vector_of(entry.slot), vector);
            prop_assert!((entry.slot as usize) < total_slots);
        }

        let mut free_slots = HashSet::new();
        for &slot in &index.free_slots {
            prop_assert!(free_slots.insert(slot));
            prop_assert!(!used_slots.contains(&slot));
            prop_assert!((slot as usize) < total_slots);
        }

        Ok(())
    }
}
