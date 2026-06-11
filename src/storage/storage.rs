use crate::format::{Frame, FrameRef, HEADER_SIZE, Header};
use crate::storage::Storage;
use crate::with_debug_log;
use std::ops::Range;

pub(crate) struct EmvedbStorage<S> {
    storage: S,
}

impl<S> EmvedbStorage<S>
where
    S: Storage,
{
    pub fn new(storage: S) -> Self {
        Self { storage }
    }

    pub fn into_inner(self) -> S {
        self.storage
    }

    pub fn sync(&mut self) -> crate::Result<()> {
        self.storage.sync()
    }

    pub fn read_at(&self, offset: u64, length: usize) -> crate::Result<Vec<u8>> {
        self.storage.read_at(offset, length)
    }

    pub fn write_header(&mut self, header: &Header) -> crate::Result<()> {
        with_debug_log! {
            self.storage
                .write_header(header)
        }
    }

    pub fn read_header(&self) -> crate::Result<Header> {
        let header = with_debug_log! {
            self
                .storage
                .read_at(0, HEADER_SIZE)
        }?;
        with_debug_log! { Header::decode(&header) }
    }

    pub fn append_frame(&mut self, frame: Frame) -> crate::Result<u64> {
        let encoded = frame.encode();
        with_debug_log! { self.storage.append(&encoded) }
    }

    pub fn read_frame(&self, offset: u64) -> crate::Result<(Range<u64>, FrameRef)> {
        let length = with_debug_log! {
            self
                .storage
                .read_at(offset, 4)
        }?;
        let length =
            u32::from_le_bytes(length.try_into().map_err(|_| crate::EmveError::Corrupt)?) as u64;

        let frame_bytes = with_debug_log! {
            self
                .storage
                .read_at(offset + 4, length as usize)
        }?;
        let frame = with_debug_log! {
            FrameRef::decode_bytes_with_length(
                &frame_bytes,
                self.read_header()?.dimension,
                length as usize,
            )
        }?;

        Ok((
            offset..offset + 4 + length,
            frame.with_body_offset(offset + 4),
        ))
    }

    pub fn read_all_frames(&self) -> crate::Result<Vec<FrameRef>> {
        let mut frames = Vec::new();
        let mut offset = HEADER_SIZE as u64;
        let storage_len = with_debug_log! {
            self
                .storage
                .len()
        }?;
        if offset >= storage_len {
            return Ok(frames);
        }
        loop {
            let (range, frame) = with_debug_log! { self.read_frame(offset) }?;
            frames.push(frame);
            offset = range.end;
            if offset >= storage_len {
                break;
            }
        }
        Ok(frames)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Metric;
    use crate::element_type::ElementType;
    use crate::format::Frame;
    use crate::storage::memory::MemoryStorage;

    #[test]
    #[tracing_test::traced_test]
    fn test_emvedb_storage() {
        let header = Header::initial(Metric::Cosine, ElementType::F32, 3);
        let mem = {
            let mut storage = EmvedbStorage::new(MemoryStorage::new_empty(header.clone()));
            let first_offset = storage
                .append_frame(Frame::Put {
                    id: 1,
                    vector: &[0.1, 0.2, 0.3],
                    payload: &b"abcde"[..],
                })
                .unwrap();
            assert_eq!(first_offset, HEADER_SIZE as u64);
            storage
                .append_frame(Frame::Put {
                    id: 1,
                    vector: &[0.4, 0.5, 0.6],
                    payload: &b"fghij"[..],
                })
                .unwrap();
            storage
                .append_frame(Frame::Put {
                    id: 1,
                    vector: &[0.7, 0.8, 0.9],
                    payload: &b"klmno"[..],
                })
                .unwrap();
            storage.into_inner().into_vec()
        };
        let storage = EmvedbStorage::new(MemoryStorage::new(mem));
        let actual_header = storage.read_header().unwrap();
        let actual_frames = storage.read_all_frames().unwrap();

        assert_eq!(header, actual_header);
        assert_eq!(3, actual_frames.len());
        match &actual_frames[0] {
            FrameRef::Put { payload_range, .. } => {
                assert_eq!(payload_range, &(94..99));
            }
            FrameRef::Delete { .. } => panic!("Expected Put frame"),
        }
    }
}
