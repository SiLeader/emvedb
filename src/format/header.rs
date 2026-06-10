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

use crate::Metric;
use crate::element_type::ElementType;
use crate::format::HeaderError;

const MAGIC: [u8; 8] = *b"EMVEDB\0\0";
pub(crate) const HEADER_SIZE: usize = 64;
const FORMAT_VERSION: u16 = 1;

#[derive(Debug, Clone, Eq, PartialEq, Hash)]
pub struct Header {
    format_version: u16,
    generation: u32,
    metric: Metric,
    element_type: ElementType,
    pub(crate) dimension: u32,
    flags: u32,
}

macro_rules! put_bytes {
    ($buf:expr, $offset:expr, $value:expr) => {{
        $buf[$offset..($offset + ::std::mem::size_of_val($value))].copy_from_slice($value);
        $offset + ::std::mem::size_of_val($value)
    }};
}

macro_rules! get_bytes {
    ($buf:expr, $offset:expr, $t:ty) => {{
        let end = $offset + ::std::mem::size_of::<$t>();
        let value = <$t>::from_le_bytes($buf[$offset..end].try_into().unwrap());
        (value, end)
    }};
}

impl Header {
    pub fn initial(metric: Metric, element_type: ElementType, dimension: u32) -> Self {
        Self {
            format_version: FORMAT_VERSION,
            generation: 0,
            metric,
            element_type,
            dimension,
            flags: 0,
        }
    }

    pub fn encode(&self) -> [u8; HEADER_SIZE] {
        let mut buf = [0u8; HEADER_SIZE];
        let mut offset = 0usize;
        offset = put_bytes!(buf, offset, &MAGIC);
        offset = put_bytes!(buf, offset, &self.format_version.to_le_bytes());
        offset = put_bytes!(buf, offset, &self.generation.to_le_bytes());
        offset = put_bytes!(buf, offset, &[self.metric.to_u8()]);
        offset = put_bytes!(buf, offset, &[self.element_type.to_u8()]);
        offset = put_bytes!(buf, offset, &self.dimension.to_le_bytes());
        offset = put_bytes!(buf, offset, &self.flags.to_le_bytes());
        let crc32c = crc32c::crc32c(&buf[..offset]);
        let _ = put_bytes!(
            buf,
            HEADER_SIZE - size_of_val(&crc32c),
            &crc32c.to_le_bytes()
        );
        buf
    }

    pub fn decode(data: &[u8]) -> crate::Result<Self> {
        if data.len() < HEADER_SIZE {
            return Err(crate::EmveError::InvalidHeader(
                HeaderError::LengthTooShort(data.len()),
            ));
        }

        let (format_version, offset) = get_bytes!(data, MAGIC.len(), u16);
        let (generation, offset) = get_bytes!(data, offset, u32);
        let (metric, offset) = get_bytes!(data, offset, u8);
        let (element_type, offset) = get_bytes!(data, offset, u8);
        let (dimension, offset) = get_bytes!(data, offset, u32);
        let (flags, offset) = get_bytes!(data, offset, u32);

        let (crc32c, _) = get_bytes!(data, HEADER_SIZE - size_of::<u32>(), u32);
        if crc32c::crc32c(&data[..offset]) != crc32c {
            return Err(crate::EmveError::Corrupt);
        }

        if format_version != FORMAT_VERSION {
            return Err(crate::EmveError::UnsupportedVersion);
        }
        let metric = Metric::from_u8(metric)
            .ok_or_else(|| crate::EmveError::InvalidHeader(HeaderError::Metric(metric)))?;
        let element_type = ElementType::from_u8(element_type).ok_or_else(|| {
            crate::EmveError::InvalidHeader(HeaderError::ElementType(element_type))
        })?;
        Ok(Self {
            format_version,
            generation,
            metric,
            element_type,
            dimension,
            flags,
        })
    }

    fn with_incremented_generation(&self) -> Self {
        Self {
            generation: self.generation.wrapping_add(1),
            ..*self
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_roundtrip() {
        let header = Header {
            format_version: FORMAT_VERSION,
            generation: 42,
            metric: Metric::L2,
            element_type: ElementType::F32,
            dimension: 128,
            flags: 0,
        };
        let encoded = header.encode();
        let decoded = Header::decode(&encoded).unwrap();
        assert_eq!(header, decoded);
    }
}
