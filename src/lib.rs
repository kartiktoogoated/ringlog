pub mod consumer;
pub mod event;
pub mod ring;
pub mod storage;

#[cfg(test)]
mod tests {
    use crate::consumer::EventConsumer;
    use crate::consumer::dispatcher::EventDispatcher;
    use crate::event::EventHeader;
    use crate::ring::RingBuffer;
    use crate::storage::{MmapReader, MmapWriter};
    use std::sync::atomic::{AtomicU64, Ordering};

    static TEST_COUNTER: AtomicU64 = AtomicU64::new(0);

    fn temp_path() -> String {
        let id = TEST_COUNTER.fetch_add(1, Ordering::SeqCst);
        format!("/tmp/ringlog_test_{}_{}.log", std::process::id(), id)
    }

    struct CountingConsumer {
        count: u64,
    }

    impl CountingConsumer {
        fn new() -> Self {
            Self { count: 0 }
        }
    }

    impl EventConsumer for CountingConsumer {
        fn consume(&mut self, _header: &EventHeader, _payload: &[u8]) -> bool {
            self.count += 1;
            true
        }

        fn name(&self) -> &str {
            "counter"
        }
    }

    struct FailingConsumer;

    impl EventConsumer for FailingConsumer {
        fn consume(&mut self, _header: &EventHeader, _payload: &[u8]) -> bool {
            false
        }

        fn name(&self) -> &str {
            "failing"
        }
    }

    mod ring_buffer {
        use super::*;

        #[test]
        fn new_creates_empty_buffer() {
            let ring = RingBuffer::new(1024);
            assert!(ring.is_empty());
            assert_eq!(ring.used(), 0);
        }

        #[test]
        fn write_single_event() {
            let mut ring = RingBuffer::new(1024);
            let header = EventHeader::new(1000, 1, 8);
            let payload = b"testdata";

            ring.write_event(&header, payload).unwrap();

            assert!(!ring.is_empty());
            assert_eq!(ring.used(), EventHeader::SIZE + 8);
        }

        #[test]
        fn read_single_event() {
            let mut ring = RingBuffer::new(1024);
            let header = EventHeader::new(1000, 1, 8);
            let payload = b"testdata";

            ring.write_event(&header, payload).unwrap();
            let (h, p) = ring.read_event().unwrap();

            assert_eq!(h.timestamp, 1000);
            assert_eq!(h.event_type, 1);
            assert_eq!(h.payload_len, 8);
            assert_eq!(&p, payload);
            assert!(ring.is_empty());
        }

        #[test]
        fn write_multiple_events() {
            let mut ring = RingBuffer::new(4096);

            for i in 0..10 {
                let header = EventHeader::new(i * 1000, 1, 4);
                ring.write_event(&header, b"test").unwrap();
            }

            let mut count = 0;
            while let Some((header, _)) = ring.read_event() {
                assert_eq!(header.timestamp, count * 1000);
                count += 1;
            }
            assert_eq!(count, 10);
        }

        #[test]
        fn buffer_full_returns_error() {
            let mut ring = RingBuffer::new(128);
            let header = EventHeader::new(0, 1, 64);
            let payload = [0u8; 64];

            let first = ring.write_event(&header, &payload);
            assert!(first.is_ok());

            let second = ring.write_event(&header, &payload);
            assert!(second.is_err());
        }

        #[test]
        fn wrap_around_works() {
            let mut ring = RingBuffer::new(256);
            let header = EventHeader::new(0, 1, 32);
            let payload = [0xAB; 32];

            for _ in 0..3 {
                ring.write_event(&header, &payload).unwrap();
            }

            for _ in 0..2 {
                ring.read_event().unwrap();
            }

            for i in 0..3 {
                let mut h = header;
                h.timestamp = i;
                ring.write_event(&h, &payload).unwrap();
            }

            let mut count = 0;
            while let Some((_, p)) = ring.read_event() {
                assert_eq!(p, payload);
                count += 1;
            }
            assert_eq!(count, 4);
        }

        #[test]
        #[should_panic]
        fn capacity_must_be_power_of_two() {
            RingBuffer::new(1000);
        }
    }

    mod event_header {
        use super::*;

        #[test]
        fn size_is_16_bytes() {
            assert_eq!(EventHeader::SIZE, 16);
        }

        #[test]
        fn total_size_includes_payload() {
            let header = EventHeader::new(0, 1, 100);
            assert_eq!(header.total_size(), 116);
        }

        #[test]
        fn new_sets_fields_correctly() {
            let header = EventHeader::new(12345, 7, 256);
            assert_eq!(header.timestamp, 12345);
            assert_eq!(header.event_type, 7);
            assert_eq!(header.payload_len, 256);
            assert_eq!(header.flags, 0);
        }
    }

    mod dispatcher {
        use super::*;

        #[test]
        fn drain_empty_buffer() {
            let mut ring = RingBuffer::new(1024);
            let mut dispatcher = EventDispatcher::new();
            dispatcher.add_consumer(CountingConsumer::new());

            let stats = dispatcher.drain(&mut ring);

            assert_eq!(stats.events_read, 0);
            assert_eq!(stats.events_delivered, 0);
        }

        #[test]
        fn drain_delivers_to_consumer() {
            let mut ring = RingBuffer::new(1024);
            let mut dispatcher = EventDispatcher::new();
            dispatcher.add_consumer(CountingConsumer::new());

            for i in 0..5 {
                let header = EventHeader::new(i, 1, 4);
                ring.write_event(&header, b"test").unwrap();
            }

            let stats = dispatcher.drain(&mut ring);

            assert_eq!(stats.events_read, 5);
            assert_eq!(stats.events_delivered, 5);
            assert_eq!(stats.events_failed, 0);
        }

        #[test]
        fn drain_tracks_failures() {
            let mut ring = RingBuffer::new(1024);
            let mut dispatcher = EventDispatcher::new();
            dispatcher.add_consumer(FailingConsumer);

            for i in 0..3 {
                let header = EventHeader::new(i, 1, 4);
                ring.write_event(&header, b"test").unwrap();
            }

            let stats = dispatcher.drain(&mut ring);

            assert_eq!(stats.events_read, 3);
            assert_eq!(stats.events_delivered, 0);
            assert_eq!(stats.events_failed, 3);
        }

        #[test]
        fn drain_batch_respects_limit() {
            let mut ring = RingBuffer::new(1024);
            let mut dispatcher = EventDispatcher::new();
            dispatcher.add_consumer(CountingConsumer::new());

            for i in 0..10 {
                let header = EventHeader::new(i, 1, 4);
                ring.write_event(&header, b"test").unwrap();
            }

            let stats = dispatcher.drain_batch(&mut ring, 3);

            assert_eq!(stats.events_read, 3);
            assert!(!ring.is_empty());
        }

        #[test]
        fn multiple_consumers() {
            let mut ring = RingBuffer::new(1024);
            let mut dispatcher = EventDispatcher::new();
            dispatcher.add_consumer(CountingConsumer::new());
            dispatcher.add_consumer(CountingConsumer::new());

            let header = EventHeader::new(0, 1, 4);
            ring.write_event(&header, b"test").unwrap();

            let stats = dispatcher.drain(&mut ring);

            assert_eq!(stats.events_read, 1);
            assert_eq!(stats.events_delivered, 2);
        }

        #[test]
        fn success_rate_calculation() {
            use crate::consumer::dispatcher::DrainStats;

            let stats = DrainStats {
                events_read: 10,
                events_delivered: 8,
                events_failed: 2,
            };

            assert!((stats.success_rate() - 0.8).abs() < 0.001);
        }

        #[test]
        fn success_rate_empty() {
            use crate::consumer::dispatcher::DrainStats;

            let stats = DrainStats::default();
            assert!((stats.success_rate() - 1.0).abs() < 0.001);
        }
    }

    mod mmap_storage {
        use super::*;
        use std::fs;

        #[test]
        fn create_and_write() {
            let path = temp_path();

            {
                let mut writer = MmapWriter::create(&path, 4096).unwrap();

                for i in 0..5 {
                    let header = EventHeader::new(i * 1000, 1, 8);
                    let payload = i.to_le_bytes();
                    assert!(writer.write_event(&header, &payload));
                }

                let fh = writer.file_header();
                assert_eq!(fh.event_count, 5);
            }

            fs::remove_file(&path).ok();
        }

        #[test]
        fn write_and_read_back() {
            let path = temp_path();

            {
                let mut writer = MmapWriter::create(&path, 4096).unwrap();

                for i in 0..10u64 {
                    let header = EventHeader::new(i * 1000, 1, 8);
                    let payload = i.to_le_bytes();
                    writer.write_event(&header, &payload);
                }

                writer.sync().unwrap();
            }

            {
                let reader = MmapReader::open(&path).unwrap();
                assert_eq!(reader.event_count(), 10);

                let mut sum = 0u64;
                let count = reader.replay(|event| {
                    let val = u64::from_le_bytes(event.payload.try_into().unwrap());
                    sum += val;
                });

                assert_eq!(count, 10);
                assert_eq!(sum, 45);
            }

            fs::remove_file(&path).ok();
        }

        #[test]
        fn iterator_works() {
            let path = temp_path();

            {
                let mut writer = MmapWriter::create(&path, 4096).unwrap();

                for i in 0..5u64 {
                    let header = EventHeader::new(i, 1, 8);
                    writer.write_event(&header, &i.to_le_bytes());
                }

                writer.sync().unwrap();
            }

            {
                let reader = MmapReader::open(&path).unwrap();
                let events: Vec<_> = reader.iter().collect();

                assert_eq!(events.len(), 5);
                for (i, event) in events.iter().enumerate() {
                    assert_eq!(event.header.timestamp, i as u64);
                }
            }

            fs::remove_file(&path).ok();
        }

        #[test]
        fn reopen_existing_file() {
            let path = temp_path();

            {
                let mut writer = MmapWriter::create(&path, 4096).unwrap();
                let header = EventHeader::new(0, 1, 4);
                writer.write_event(&header, b"test");
                writer.sync().unwrap();
            }

            {
                let mut writer = MmapWriter::open(&path).unwrap();
                let header = EventHeader::new(1, 1, 4);
                writer.write_event(&header, b"more");
                writer.sync().unwrap();
            }

            {
                let reader = MmapReader::open(&path).unwrap();
                assert_eq!(reader.event_count(), 2);
            }

            fs::remove_file(&path).ok();
        }

        #[test]
        fn buffer_full_returns_false() {
            let path = temp_path();

            {
                let mut writer = MmapWriter::create(&path, 4096).unwrap();
                let header = EventHeader::new(0, 1, 2048);
                let payload = [0u8; 2048];

                let first = writer.write_event(&header, &payload);
                assert!(first);

                let second = writer.write_event(&header, &payload);
                assert!(!second);
            }

            fs::remove_file(&path).ok();
        }

        #[test]
        fn invalid_file_returns_error() {
            let path = temp_path();
            fs::write(&path, b"not a valid log file").unwrap();

            let result = MmapReader::open(&path);
            assert!(result.is_err());

            fs::remove_file(&path).ok();
        }
    }
}
