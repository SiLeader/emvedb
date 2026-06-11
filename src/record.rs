#[derive(Debug)]
pub struct Record {
    id: u64,
    vector: Vec<f32>,
    payload: Vec<u8>,
}

impl Record {
    pub fn new(id: u64, vector: Vec<f32>, payload: Vec<u8>) -> Self {
        Self {
            id,
            vector,
            payload,
        }
    }

    pub fn id(&self) -> u64 {
        self.id
    }
    pub fn vector(&self) -> &[f32] {
        &self.vector
    }
    pub fn payload(&self) -> &[u8] {
        &self.payload
    }
}
