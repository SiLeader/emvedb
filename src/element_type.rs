#[derive(Debug, Clone, Copy, Eq, PartialEq, Hash)]
pub enum ElementType {
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
