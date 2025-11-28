#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct FileHeader {
    pub magic: [u8; 4],
    pub version: u32,
    pub created_at: i64,
    pub event_count: u64,
    pub write_offset: u64,
    pub _reserved: [u8; 32],
}

impl FileHeader {
    pub const SIZE: usize = 64;
    pub const MAGIC: [u8; 4] = *b"EVIL";
    pub const VERSION: u32 = 1;

    pub fn new(created_at: i64) -> Self {
        Self {
            magic: Self::MAGIC,
            version: Self::VERSION,
            created_at,
            event_count: 0,
            write_offset: Self::SIZE as u64,
            _reserved: [0; 32],
        }
    }

    #[inline]
    pub fn validate(&self) -> bool {
        self.magic == Self::MAGIC && self.version == Self::VERSION
    }
}
