/// Vector element type stored in a database.
///
/// EmveDB currently supports only `f32` vectors.
#[derive(Debug, Clone, Copy, Eq, PartialEq, Hash)]
pub enum ElementType {
    /// 32-bit floating-point vector elements.
    F32,
}

impl ElementType {
    pub(crate) const fn to_u8(self) -> u8 {
        match self {
            ElementType::F32 => 0,
        }
    }

    pub(crate) const fn from_u8(value: u8) -> Option<Self> {
        match value {
            0 => Some(ElementType::F32),
            _ => None,
        }
    }
}
