use crate::event::EventHeader;
use std::cell::UnsafeCell;
use std::sync::atomic::{AtomicUsize, Ordering};
pub struct SpscRingBuffer {
    buf: UnsafeCell<Box<[u8]>>,
    capacity: usize,
    mask: usize,
    head: AtomicUsize,
    tail: AtomicUsize,
}
unsafe impl Send for SpscRingBuffer {}
unsafe impl Sync for SpscRingBuffer {}
impl SpscRingBuffer {
    pub fn new(capacity: usize) -> Self {
        assert!(capacity.is_power_of_two());
        assert!(capacity >= 64);
        Self {
            buf: UnsafeCell::new(vec![0u8; capacity].into_boxed_slice()),
            capacity,
            mask: capacity - 1,
            head: AtomicUsize::new(0),
            tail: AtomicUsize::new(0),
        }
    }
    pub fn split(&self) -> (Producer<'_>, Consumer<'_>) {
        (Producer { ring: self }, Consumer { ring: self })
    }
    #[inline]
    pub fn is_empty(&self) -> bool {
        self.head.load(Ordering::Relaxed) == self.tail.load(Ordering::Relaxed)
    }
}
pub struct Producer<'a> {
    ring: &'a SpscRingBuffer,
}
pub struct Consumer<'a> {
    ring: &'a SpscRingBuffer,
}
impl Producer<'_> {
    #[inline]
    pub fn write_event(&mut self, header: &EventHeader, payload: &[u8]) -> bool {
        let total_size = header.total_size();
        let head = self.ring.head.load(Ordering::Relaxed);
        let tail = self.ring.tail.load(Ordering::Acquire);
        let available = self.ring.capacity - head.wrapping_sub(tail) - 1;
        if total_size > available {
            return false;
        }
        let mask = self.ring.mask;
        let start = head & mask;
        let contiguous = self.ring.capacity - start;
        unsafe {
            let buf = &mut *self.ring.buf.get();
            let buf_ptr = buf.as_mut_ptr();
            if total_size <= contiguous {
                std::ptr::write_unaligned(buf_ptr.add(start) as *mut EventHeader, *header);
                std::ptr::copy_nonoverlapping(
                    payload.as_ptr(),
                    buf_ptr.add(start + EventHeader::SIZE),
                    payload.len(),
                );
            } else if contiguous >= EventHeader::SIZE {
                std::ptr::write_unaligned(buf_ptr.add(start) as *mut EventHeader, *header);
                let first_chunk = contiguous - EventHeader::SIZE;
                if first_chunk > 0 {
                    std::ptr::copy_nonoverlapping(
                        payload.as_ptr(),
                        buf_ptr.add(start + EventHeader::SIZE),
                        first_chunk,
                    );
                }
                std::ptr::copy_nonoverlapping(
                    payload.as_ptr().add(first_chunk),
                    buf_ptr,
                    payload.len() - first_chunk,
                );
            } else {
                let header_bytes =
                    &*(header as *const EventHeader as *const [u8; EventHeader::SIZE]);
                std::ptr::copy_nonoverlapping(
                    header_bytes.as_ptr(),
                    buf_ptr.add(start),
                    contiguous,
                );
                std::ptr::copy_nonoverlapping(
                    header_bytes.as_ptr().add(contiguous),
                    buf_ptr,
                    EventHeader::SIZE - contiguous,
                );
                std::ptr::copy_nonoverlapping(
                    payload.as_ptr(),
                    buf_ptr.add(EventHeader::SIZE - contiguous),
                    payload.len(),
                );
            }
        }
        self.ring
            .head
            .store(head.wrapping_add(total_size), Ordering::Release);
        true
    }
}
impl Consumer<'_> {
    #[inline]
    pub fn read_event(&mut self) -> Option<(EventHeader, Vec<u8>)> {
        let tail = self.ring.tail.load(Ordering::Relaxed);
        let head = self.ring.head.load(Ordering::Acquire);
        if head == tail {
            return None;
        }
        let mask = self.ring.mask;
        let start = tail & mask;
        let contiguous = self.ring.capacity - start;
        unsafe {
            let buf = &*self.ring.buf.get();
            let buf_ptr = buf.as_ptr();
            let header = if contiguous >= EventHeader::SIZE {
                std::ptr::read_unaligned(buf_ptr.add(start) as *const EventHeader)
            } else {
                let mut header_bytes = [0u8; EventHeader::SIZE];
                std::ptr::copy_nonoverlapping(
                    buf_ptr.add(start),
                    header_bytes.as_mut_ptr(),
                    contiguous,
                );
                std::ptr::copy_nonoverlapping(
                    buf_ptr,
                    header_bytes.as_mut_ptr().add(contiguous),
                    EventHeader::SIZE - contiguous,
                );
                std::ptr::read_unaligned(header_bytes.as_ptr() as *const EventHeader)
            };
            let payload_len = header.payload_len as usize;
            let mut payload = vec![0u8; payload_len];
            let payload_start = (start + EventHeader::SIZE) & mask;
            let payload_contiguous = self.ring.capacity - payload_start;
            if payload_len <= payload_contiguous {
                std::ptr::copy_nonoverlapping(
                    buf_ptr.add(payload_start),
                    payload.as_mut_ptr(),
                    payload_len,
                );
            } else {
                std::ptr::copy_nonoverlapping(
                    buf_ptr.add(payload_start),
                    payload.as_mut_ptr(),
                    payload_contiguous,
                );
                std::ptr::copy_nonoverlapping(
                    buf_ptr,
                    payload.as_mut_ptr().add(payload_contiguous),
                    payload_len - payload_contiguous,
                );
            }
            let total_size = header.total_size();
            self.ring
                .tail
                .store(tail.wrapping_add(total_size), Ordering::Release);
            Some((header, payload))
        }
    }
    #[inline]
    pub fn is_empty(&self) -> bool {
        self.ring.is_empty()
    }
}
