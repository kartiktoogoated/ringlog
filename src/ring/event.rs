use super::RingError;
use crate::event::EventHeader;
use crate::ring::RingBuffer;
use std::ptr;

impl RingBuffer {
    pub fn new(capacity: usize) -> Result<Self, RingError> {
        if !capacity.is_power_of_two() {
            return Err(RingError::InvalidCapacity {
                capacity,
                reason: "must be a power of two",
            });
        }
        
        let min_capacity = EventHeader::SIZE * 2;
        if capacity < min_capacity {
            return Err(RingError::InvalidCapacity {
                capacity,
                reason: "too small, must be at least 2x EventHeader::SIZE",
            });
        }
        
        Ok(Self {
            buf: vec![0; capacity],
            capacity,
            head: 0,
            tail: 0,
        })
    }

    #[inline(always)]
    pub fn used(&self) -> usize {
        self.head.wrapping_sub(self.tail) & (self.capacity - 1)
    }

    #[inline(always)]
    pub fn available(&self) -> usize {
        self.capacity - self.used() - 1
    }

    #[inline(always)]
    pub fn is_empty(&self) -> bool {
        self.head == self.tail
    }

    #[inline]
    pub fn write_event(&mut self, header: &EventHeader, payload: &[u8]) -> Result<(), RingError> {
        let total_size = header.total_size();
        let available = self.available();
        
        if total_size > available {
            return Err(RingError::NotEnoughSpace {
                required: total_size,
                available,
            });
        }

        let mask = self.capacity - 1;
        let start = self.head;
        let contiguous_space = self.capacity - start;

        unsafe {
            let buf_ptr = self.buf.as_mut_ptr();

            if total_size <= contiguous_space {
                ptr::write_unaligned(buf_ptr.add(start) as *mut EventHeader, *header);
                ptr::copy_nonoverlapping(
                    payload.as_ptr(),
                    buf_ptr.add(start + EventHeader::SIZE),
                    payload.len(),
                );
            } else if contiguous_space >= EventHeader::SIZE {
                ptr::write_unaligned(buf_ptr.add(start) as *mut EventHeader, *header);
                let first_chunk = contiguous_space - EventHeader::SIZE;
                ptr::copy_nonoverlapping(
                    payload.as_ptr(),
                    buf_ptr.add(start + EventHeader::SIZE),
                    first_chunk,
                );
                ptr::copy_nonoverlapping(
                    payload.as_ptr().add(first_chunk),
                    buf_ptr,
                    payload.len() - first_chunk,
                );
            } else {
                let header_bytes =
                    &*(header as *const EventHeader as *const [u8; EventHeader::SIZE]);
                ptr::copy_nonoverlapping(
                    header_bytes.as_ptr(),
                    buf_ptr.add(start),
                    contiguous_space,
                );
                ptr::copy_nonoverlapping(
                    header_bytes.as_ptr().add(contiguous_space),
                    buf_ptr,
                    EventHeader::SIZE - contiguous_space,
                );
                ptr::copy_nonoverlapping(
                    payload.as_ptr(),
                    buf_ptr.add(EventHeader::SIZE - contiguous_space),
                    payload.len(),
                );
            }
        }

        self.head = (start + total_size) & mask;
        Ok(())
    }

    #[inline]
    pub fn read_event(&mut self) -> Option<(EventHeader, Vec<u8>)> {
        if self.is_empty() {
            return None;
        }

        let mask = self.capacity - 1;
        let start = self.tail;
        let contiguous = self.capacity - start;

        unsafe {
            let buf_ptr = self.buf.as_ptr();

            let header = if contiguous >= EventHeader::SIZE {
                ptr::read_unaligned(buf_ptr.add(start) as *const EventHeader)
            } else {
                let mut header_bytes = [0u8; EventHeader::SIZE];
                ptr::copy_nonoverlapping(buf_ptr.add(start), header_bytes.as_mut_ptr(), contiguous);
                ptr::copy_nonoverlapping(
                    buf_ptr,
                    header_bytes.as_mut_ptr().add(contiguous),
                    EventHeader::SIZE - contiguous,
                );
                ptr::read_unaligned(header_bytes.as_ptr() as *const EventHeader)
            };

            let payload_len = header.payload_len as usize;
            let mut payload = vec![0u8; payload_len];

            let payload_start = (start + EventHeader::SIZE) & mask;
            let payload_contiguous = self.capacity - payload_start;

            if payload_len <= payload_contiguous {
                ptr::copy_nonoverlapping(
                    buf_ptr.add(payload_start),
                    payload.as_mut_ptr(),
                    payload_len,
                );
            } else {
                ptr::copy_nonoverlapping(
                    buf_ptr.add(payload_start),
                    payload.as_mut_ptr(),
                    payload_contiguous,
                );
                ptr::copy_nonoverlapping(
                    buf_ptr,
                    payload.as_mut_ptr().add(payload_contiguous),
                    payload_len - payload_contiguous,
                );
            }

            self.tail = (start + header.total_size()) & mask;

            Some((header, payload))
        }
    }
}
