/// A live record returned by [`EmveDb::get`](crate::EmveDb::get).
///
/// Records own their vector and payload data. Use the accessor methods to read
/// those values without taking ownership.
#[derive(Debug)]
pub struct Record {
    id: u64,
    vector: Vec<f32>,
    payload: Vec<u8>,
}

impl Record {
    /// Creates a record from owned values.
    ///
    /// This constructor does not validate vector dimension or contents.
    pub fn new(id: u64, vector: Vec<f32>, payload: Vec<u8>) -> Self {
        Self {
            id,
            vector,
            payload,
        }
    }

    /// Returns the record id.
    pub fn id(&self) -> u64 {
        self.id
    }
    /// Returns the vector contents.
    pub fn vector(&self) -> &[f32] {
        &self.vector
    }
    /// Returns the opaque payload bytes.
    pub fn payload(&self) -> &[u8] {
        &self.payload
    }
}
