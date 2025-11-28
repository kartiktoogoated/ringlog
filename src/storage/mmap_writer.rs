use super::FileHeader;
use crate::event::EventHeader;
use std::fs::{File, OpenOptions};
use std::io;
use std::path::Path;
use std::ptr;

pub struct MmapWriter {
    _file: File,
    mmap_ptr: *mut u8,
    mmap_len: usize,
    write_offset: usize,
}

impl MmapWriter {
    pub fn create<P: AsRef<Path>>(path: P, capacity: usize) -> io::Result<Self> {
        let capacity = capacity.max(4096);

        let file = OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(true)
            .open(path)?;

        let _ = file.set_len(capacity as u64);

        let mmap_ptr = unsafe {
            libc::mmap(
                ptr::null_mut(),
                capacity,
                libc::PROT_READ | libc::PROT_WRITE,
                libc::MAP_SHARED,
                std::os::unix::io::AsRawFd::as_raw_fd(&file),
                0,
            )
        };

        if mmap_ptr == libc::MAP_FAILED {
            return Err(io::Error::last_os_error());
        }

        let mut mmap_writer = Self {
            _file: file,
            mmap_ptr: mmap_ptr as *mut u8,
            mmap_len: capacity,
            write_offset: FileHeader::SIZE,
        };

        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs() as i64;

        let header = FileHeader::new(now);
        mmap_writer.write_file_header(&header);

        Ok(mmap_writer)
    }

    pub fn open<P: AsRef<Path>>(path: P) -> io::Result<Self> {
        let file = OpenOptions::new().read(true).write(true).open(path)?;

        let metadata = file.metadata()?;
        let capacity = metadata.len() as usize;

        let mmap_ptr = unsafe {
            libc::mmap(
                ptr::null_mut(),
                capacity,
                libc::PROT_READ | libc::PROT_WRITE,
                libc::MAP_SHARED,
                std::os::unix::io::AsRawFd::as_raw_fd(&file),
                0,
            )
        };
        if mmap_ptr == libc::MAP_FAILED {
            return Err(io::Error::last_os_error());
        }

        let header = unsafe { ptr::read_unaligned(mmap_ptr as *const FileHeader) };

        if !header.validate() {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "Invalid file header",
            ));
        }

        Ok(Self {
            _file: file,
            mmap_ptr: mmap_ptr as *mut u8,
            mmap_len: capacity,
            write_offset: header.write_offset as usize,
        })
    }

    #[inline]
    pub fn available(&self) -> usize {
        self.mmap_len - self.write_offset
    }

    #[inline]
    pub fn write_event(&mut self, header: &EventHeader, payload: &[u8]) -> bool {
        let total_size = header.total_size();

        if total_size > self.available() {
            return false;
        }

        unsafe {
            let dst = self.mmap_ptr.add(self.write_offset);

            ptr::write_unaligned(dst as *mut EventHeader, *header);

            ptr::copy_nonoverlapping(payload.as_ptr(), dst.add(EventHeader::SIZE), payload.len());
        }

        self.write_offset += total_size;
        self.update_file_header();

        true
    }

    pub fn sync(&self) -> io::Result<()> {
        let result = unsafe {
            libc::msync(
                self.mmap_ptr as *mut libc::c_void,
                self.mmap_len,
                libc::MS_SYNC,
            )
        };

        if result == 0 {
            Ok(())
        } else {
            Err(io::Error::last_os_error())
        }
    }

    pub fn sync_async(&self) -> io::Result<()> {
        let result = unsafe {
            libc::msync(
                self.mmap_ptr as *mut libc::c_void,
                self.mmap_len,
                libc::MS_ASYNC,
            )
        };

        if result == 0 {
            Ok(())
        } else {
            Err(io::Error::last_os_error())
        }
    }

    #[inline]
    pub fn write_offset(&self) -> usize {
        self.write_offset
    }

    pub fn file_header(&self) -> FileHeader {
        unsafe { ptr::read_unaligned(self.mmap_ptr as *const FileHeader) }
    }

    #[inline]
    fn write_file_header(&mut self, header: &FileHeader) {
        unsafe {
            ptr::write_unaligned(self.mmap_ptr as *mut FileHeader, *header);
        }
    }

    #[inline]
    fn update_file_header(&mut self) {
        unsafe {
            let header = &mut *(self.mmap_ptr as *mut FileHeader);
            header.event_count += 1;
            header.write_offset = self.write_offset as u64;
        }
    }
}

impl Drop for MmapWriter {
    fn drop(&mut self) {
        let _ = self.sync();

        unsafe {
            libc::munmap(self.mmap_ptr as *mut libc::c_void, self.mmap_len);
        }
    }
}

unsafe impl Send for MmapWriter {}
