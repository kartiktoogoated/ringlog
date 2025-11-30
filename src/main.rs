use ringlog::consumer::EventConsumer;
use ringlog::consumer::dispatcher::EventDispatcher;
use ringlog::event::EventHeader;
use ringlog::ring::RingBuffer;
use ringlog::storage::MmapWriter;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

struct MmapConsumer {
    writer: MmapWriter,
    events_written: u64,
}

impl MmapConsumer {
    fn new(path: &str, capacity: usize) -> std::io::Result<Self> {
        Ok(Self {
            writer: MmapWriter::create(path, capacity)?,
            events_written: 0,
        })
    }
}

impl EventConsumer for MmapConsumer {
    fn consume(&mut self, header: &EventHeader, payload: &[u8]) -> bool {
        let ok = self.writer.write_event(header, payload);
        if ok {
            self.events_written += 1;
        }
        ok
    }

    fn flush(&mut self) {
        let _ = self.writer.sync_async();
    }

    fn name(&self) -> &str {
        "mmap"
    }
}

fn main() {
    if let Err(e) = run() {
        eprintln!("Error: {}", e);
        std::process::exit(1);
    }
}

fn run() -> Result<(), Box<dyn std::error::Error>> {
    println!("ringlog v0.1.0");
    println!("Press Ctrl+C to stop\n");

    let running = Arc::new(AtomicBool::new(true));
    let r = running.clone();

    ctrlc::set_handler(move || {
        println!("\nShutting down...");
        r.store(false, Ordering::SeqCst);
    })
    .map_err(|e| format!("Failed to set Ctrl+C handler: {}", e))?;

    let mut ring = RingBuffer::new(64 * 1024)
        .map_err(|e| format!("Failed to create ring buffer: {}", e))?;
    let mut dispatcher = EventDispatcher::new();

    let mmap_consumer = MmapConsumer::new("/tmp/ringlog.log", 64 * 1024 * 1024)
        .map_err(|e| format!("Failed to create mmap consumer: {}", e))?;
    dispatcher.add_consumer(mmap_consumer);

    let mut total_events = 0u64;
    let mut last_report = Instant::now();

    println!("Service running. Waiting for events...");

    while running.load(Ordering::SeqCst) {
        let stats = dispatcher.drain(&mut ring);
        total_events += stats.events_read;

        if last_report.elapsed() >= Duration::from_secs(5) {
            println!(
                "[STATUS] total_events={} ring_used={} ring_available={}",
                total_events,
                ring.used(),
                ring.available()
            );
            last_report = Instant::now();
        }

        std::thread::sleep(Duration::from_millis(10));
    }

    println!("Total events processed: {}", total_events);
    
    if let Err(e) = std::fs::remove_file("/tmp/ringlog.log") {
        eprintln!("Warning: Failed to remove temporary file: {}", e);
    }
    
    Ok(())
}
