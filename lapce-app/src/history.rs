use lapce_core::buffer::Buffer;

#[derive(Clone)]
pub struct DocumentHistory {
    pub buffer: Buffer,
}

impl DocumentHistory {
    pub fn new(content: &str) -> Self {
        Self {
            buffer: Buffer::new(content),
        }
    }
}
