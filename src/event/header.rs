#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct EventHeader {
    pub timestamp: u64,
    pub event_type: u8,
    pub flags: u8,
    pub payload_len: u16,
    pub _reserved: u32,
}

impl EventHeader {
    pub const SIZE: usize = 16;

    pub fn new(timestamp: u64, event_type: u8, payload_len: u16) -> Self {
        Self {
            timestamp,
            event_type,
            flags: 0,
            payload_len,
            _reserved: 0,
        }
    }

    pub fn total_size(&self) -> usize {
        Self::SIZE + self.payload_len as usize
    }
}
