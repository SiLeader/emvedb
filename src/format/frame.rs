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

use crate::format::FrameError;
use crate::with_debug_log;
use std::io::Cursor;

pub(crate) enum Frame<'a> {
    Put {
        id: u64,
        vector: &'a [f32],
        payload: &'a [u8],
    },
    Delete {
        id: u64,
    },
}

#[derive(Debug)]
pub(crate) enum FrameRef {
    Put {
        id: u64,
        vector_range: std::ops::Range<u64>,
        payload_range: std::ops::Range<u64>,
    },
    Delete {
        id: u64,
    },
}

impl Frame<'_> {
    pub(crate) fn encode(&self) -> Vec<u8> {
        let body = match self {
            Frame::Put {
                id,
                vector,
                payload,
            } => encode_put(*id, *vector, *payload),
            Frame::Delete { id } => encode_delete(*id),
        };
        let mut buf = Vec::with_capacity(13 + body.len());
        buf.extend_from_slice(&((body.len() + 4) as u32).to_le_bytes());

        let crc = crc32c::crc32c(&body);
        buf.extend(body);
        buf.extend_from_slice(&crc.to_le_bytes());
        buf
    }
}

macro_rules! get_bytes {
    ($buf:expr, $offset:expr, $t:ty) => {{
        let end = $offset + std::mem::size_of::<$t>();
        let value =
            <$t>::from_le_bytes($buf[$offset..end].try_into().map_err(|_| {
                crate::EmveError::InvalidFrame(FrameError::ByteRange($offset, end))
            })?);
        (value, end)
    }};
}
macro_rules! read_bytes {
    ($buf:expr, $t:ty) => {{
        let mut buffer = [0u8; std::mem::size_of::<$t>()];
        $buf.read(&mut buffer)?;
        let value = <$t>::from_le_bytes(buffer);
        value
    }};
}

impl FrameRef {
    pub(crate) fn decode_bytes(data: &[u8], dimension: u32) -> crate::Result<Self> {
        let mut cursor = Cursor::new(data);
        Self::decode(&mut cursor, dimension)
    }

    pub(crate) fn decode_bytes_with_length(
        data: &[u8],
        dimension: u32,
        body_len: usize,
    ) -> crate::Result<Self> {
        let mut cursor = Cursor::new(data);
        Self::decode_with_length(&mut cursor, dimension, body_len)
    }

    pub(crate) fn decode_with_length(
        data: &mut impl std::io::Read,
        dimension: u32,
        body_len: usize,
    ) -> crate::Result<Self> {
        let mut body_data = Vec::with_capacity(body_len - 4);
        body_data.resize(body_len - 4, 0);
        data.read_exact(&mut body_data)?;

        let crc = read_bytes!(data, u32);
        let actual_crc = crc32c::crc32c(&body_data);
        if crc != actual_crc {
            return with_debug_log! {
                Err(crate::EmveError::InvalidFrame(FrameError::CrcMismatch {
                    expected: crc,
                    actual: actual_crc,
                }))
            };
        }
        match body_data[0] {
            1 => decode_put::<f32>(&body_data, dimension),
            2 => decode_delete(&body_data),
            _ => with_debug_log! {
                Err(crate::EmveError::InvalidFrame(FrameError::FrameType(
                    body_data[0],
                )))
            },
        }
    }

    pub(crate) fn decode(data: &mut impl std::io::Read, dimension: u32) -> crate::Result<Self> {
        let body_len = read_bytes!(data, u32) as usize;
        Self::decode_with_length(data, dimension, body_len)
    }
}

fn encode_put(id: u64, vector: &[f32], payload: &[u8]) -> Vec<u8> {
    let mut buf = Vec::with_capacity(
        // type = 1
        1 +
        // flags
        1 +
        // id
        8 +
        // payload length
        4 +
        // vector
        (vector.len() * 4) +
        // payload
        payload.len(),
    );
    buf.push(1u8);
    buf.push(0u8);
    buf.extend(id.to_le_bytes());
    buf.extend((payload.len() as u32).to_le_bytes());
    buf.extend(vector.iter().flat_map(|&x| x.to_le_bytes()));
    buf.extend(payload);
    buf
}

fn decode_put<V>(data: &[u8], dimension: u32) -> Result<FrameRef, crate::EmveError> {
    let (id, offset) = get_bytes!(data, 2, u64);
    let (payload_len, offset) = get_bytes!(data, offset, u32);

    let vector_len = dimension as usize * size_of::<V>();
    let payload_offset = offset + vector_len;

    let vector_range = (offset as u64)..(offset as u64 + vector_len as u64);
    let payload_range = (payload_offset as u64)..(payload_offset as u64 + payload_len as u64);
    Ok(FrameRef::Put {
        id,
        vector_range,
        payload_range,
    })
}

fn encode_delete(id: u64) -> Vec<u8> {
    let mut buf = Vec::with_capacity(1 + 1 + 8);
    buf.push(2u8);
    buf.push(0u8);
    buf.extend(id.to_le_bytes());
    buf
}

fn decode_delete(data: &[u8]) -> Result<FrameRef, crate::EmveError> {
    let (id, _offset) = get_bytes!(data, 2, u64);
    Ok(FrameRef::Delete { id })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_roundtrip_put() {
        let put = Frame::Put {
            id: 42,
            vector: &[1.0, 2.0, 3.0],
            payload: b"hello",
        };
        let encoded = put.encode();
        let decoded = FrameRef::decode_bytes(&encoded, 3).unwrap();
        match decoded {
            FrameRef::Put {
                id,
                vector_range,
                payload_range,
            } => {
                assert_eq!(id, 42);
                assert_eq!(vector_range, 14..26); // 3 floats * 4 bytes each
                assert_eq!(payload_range, 26..31); // "hello" is 5 bytes
            }
            _ => panic!("Expected Put frame"),
        }
    }

    #[test]
    fn test_roundtrip_delete() {
        let delete = Frame::Delete { id: 42 };
        let encoded = delete.encode();
        let decoded = FrameRef::decode_bytes(&encoded, 0).unwrap();
        match decoded {
            FrameRef::Delete { id } => {
                assert_eq!(id, 42);
            }
            _ => panic!("Expected Delete frame"),
        }
    }

    #[test]
    fn test_detect_invalid_crc_put() {
        let put = Frame::Put {
            id: 42,
            vector: &[1.0, 2.0, 3.0],
            payload: b"hello",
        };
        let mut encoded = put.encode();
        let last_bytes = encoded.last_mut().unwrap();
        *last_bytes ^= 0b00000100; // flip bit
        FrameRef::decode_bytes(&encoded, 3).unwrap_err();
    }
}
