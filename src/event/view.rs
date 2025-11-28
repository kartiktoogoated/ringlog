use super::EventHeader;

#[derive(Debug, Clone, Copy)]
pub struct EventView<'a> {
    pub header: &'a EventHeader,
    pub payload: &'a [u8],
}

impl<'a> EventView<'a> {
    /// # Safety
    /// Caller must guarantee that `buf[offset..]` contains a valid
    pub unsafe fn from_bytes(buf: &'a [u8], offset: usize) -> Self {
        let header = {
            let ptr = unsafe { buf.as_ptr().add(offset) as *const EventHeader };
            unsafe { &*ptr }
        };

        let ps = offset + EventHeader::SIZE;
        let pe = ps + header.payload_len as usize;

        let payload = &buf[ps..pe];

        Self { header, payload }
    }

    pub fn total_size(&self) -> usize {
        self.header.total_size()
    }
}
