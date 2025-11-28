use super::FileHeader;
use crate::event::{EventHeader, EventView};
use std::fs::File;
use std::io;
use std::os::unix::io::AsRawFd;
use std::path::Path;
use std::ptr;

pub struct MmapReader {
    _file: File,
    mmap_ptr: *const u8,
    mmap_len: usize,
    file_header: FileHeader,
}

impl MmapReader {
    pub fn open<P: AsRef<Path>>(path: P) -> io::Result<Self> {
        let file = File::open(path)?;
        let metadata = file.metadata()?;
        let len = metadata.len() as usize;

        if len < FileHeader::SIZE {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "File too small for header",
            ));
        }

        let mmap_ptr = unsafe {
            libc::mmap(
                ptr::null_mut(),
                len,
                libc::PROT_READ,
                libc::MAP_SHARED,
                file.as_raw_fd(),
                0,
            )
        };

        if mmap_ptr == libc::MAP_FAILED {
            return Err(io::Error::last_os_error());
        }

        let file_header = unsafe { ptr::read_unaligned(mmap_ptr as *const FileHeader) };

        if !file_header.validate() {
            unsafe {
                libc::munmap(mmap_ptr, len);
            }
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "Invalid file header",
            ));
        }

        Ok(Self {
            _file: file,
            mmap_ptr: mmap_ptr as *const u8,
            mmap_len: len,
            file_header,
        })
    }

    #[inline]
    pub fn event_count(&self) -> u64 {
        self.file_header.event_count
    }

    #[inline]
    pub fn created_at(&self) -> i64 {
        self.file_header.created_at
    }

    #[inline]
    pub fn replay<F>(&self, mut callback: F) -> u64
    where
        F: FnMut(EventView),
    {
        let mut offset = FileHeader::SIZE;
        let end = self.file_header.write_offset as usize;
        let mut count = 0;

        while offset < end {
            let event = self.event_at(offset);
            let size = event.total_size();
            callback(event);
            offset += size;
            count += 1;
        }

        count
    }

    #[inline]
    fn event_at(&self, offset: usize) -> EventView<'_> {
        unsafe {
            let header_ptr = self.mmap_ptr.add(offset) as *const EventHeader;
            let header = &*header_ptr;

            let payload_ptr = self.mmap_ptr.add(offset + EventHeader::SIZE);
            let payload = std::slice::from_raw_parts(payload_ptr, header.payload_len as usize);

            EventView { header, payload }
        }
    }

    pub fn iter(&self) -> EventIterator<'_> {
        EventIterator {
            reader: self,
            offset: FileHeader::SIZE,
            end: self.file_header.write_offset as usize,
        }
    }

    pub fn advise_sequential(&self) {
        unsafe {
            libc::madvise(
                self.mmap_ptr as *mut libc::c_void,
                self.mmap_len,
                libc::MADV_SEQUENTIAL,
            );
        }
    }

    pub fn advise_willneed(&self) {
        unsafe {
            libc::madvise(
                self.mmap_ptr as *mut libc::c_void,
                self.mmap_len,
                libc::MADV_WILLNEED,
            );
        }
    }
}

impl Drop for MmapReader {
    fn drop(&mut self) {
        unsafe {
            libc::munmap(self.mmap_ptr as *mut libc::c_void, self.mmap_len);
        }
    }
}

unsafe impl Send for MmapReader {}
unsafe impl Sync for MmapReader {}

pub struct EventIterator<'a> {
    reader: &'a MmapReader,
    offset: usize,
    end: usize,
}

impl<'a> Iterator for EventIterator<'a> {
    type Item = EventView<'a>;

    fn next(&mut self) -> Option<Self::Item> {
        if self.offset >= self.end {
            return None;
        }

        let event = self.reader.event_at(self.offset);
        self.offset += event.total_size();
        Some(event)
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        let max_events = (self.end - self.offset) / EventHeader::SIZE;
        (0, Some(max_events))
    }
}
