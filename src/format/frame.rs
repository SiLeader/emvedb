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

use std::process::id;

enum Frame<'a> {
    Put {
        id: u64,
        vector: &'a [f32],
        payload: &'a [u8],
    },
    Delete {
        id: u64,
    },
}

enum FrameRef {
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
    fn encode(&self) -> Vec<u8> {
        let body = match self {
            Frame::Put {
                id,
                vector,
                payload,
            } => encode_put(*id, *vector, *payload),
            Frame::Delete { id } => encode_delete(*id),
        };
        let mut buf = Vec::with_capacity(13 + body.len());
        buf.extend_from_slice(&(body.len() as u32).to_le_bytes());
        buf.extend(body);
        let crc = crc32c::crc32c(&buf);
        buf.extend_from_slice(&crc.to_le_bytes());
        buf
    }

    fn decode(data: &[u8], dimension: u32) -> crate::Result<Self> {
        match data[0] {
            1 => {
                todo!()
            }
            2 => {
                let id = u64::from_le_bytes(data[1..9].try_into().unwrap());
                Ok(Frame::Delete { id })
            }
            _ => todo!(),
        }
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
    buf.push(1);
    buf.push(0);
    buf.extend(id.to_le_bytes());
    buf.extend((payload.len() as u32).to_le_bytes());
    buf.extend(vector.iter().flat_map(|&x| x.to_le_bytes()));
    buf.extend(payload);
    buf
}
fn encode_delete(id: u64) -> Vec<u8> {
    let mut buf = Vec::with_capacity(1 + 1 + 8);
    buf.push(2);
    buf.push(0);
    buf.extend(id.to_le_bytes());
    buf
}
