use ringlog::event::EventHeader;
use ringlog::ring::SpscRingBuffer;
use ringlog::storage::MmapWriter;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

fn main() {
    if let Err(e) = run() {
        eprintln!("Error: {}", e);
        std::process::exit(1);
    }
}

fn run() -> Result<(), Box<dyn std::error::Error>> {
    println!("SPSC ringlog stress test + mmap\n");

    let running = Arc::new(AtomicBool::new(true));
    let r = running.clone();

    ctrlc::set_handler(move || {
        r.store(false, Ordering::SeqCst);
    })
    .map_err(|e| format!("Failed to set Ctrl+C handler: {}", e))?;

    let ring = SpscRingBuffer::new(64 * 1024 * 1024)
        .map_err(|e| format!("Failed to create SPSC ring buffer: {}", e))?;
    let (mut prod, mut cons) = ring.split();

    std::thread::scope(|scope| {
        let writer_running = running.clone();
        let writer = scope.spawn(move || {
            let mut count = 0u64;
            let payload = [0u8; 64];

            while writer_running.load(Ordering::Relaxed) {
                let header = EventHeader::new(count, 1, 64);
                if prod.write_event(&header, &payload) {
                    count += 1;
                }
            }

            count
        });

        let reader_running = running.clone();
        let reader = scope.spawn(move || -> Result<u64, std::io::Error> {
            let mut mmap = MmapWriter::create("/tmp/ringlog_stress.log", 1024 * 1024 * 1024)?;
            let mut count = 0u64;

            loop {
                while let Some((header, payload)) = cons.read_event() {
                    mmap.write_event(&header, &payload);
                    count += 1;
                }

                if !reader_running.load(Ordering::Relaxed) && cons.is_empty() {
                    break;
                }
            }

            mmap.sync()?;
            Ok(count)
        });

        println!("Running for 5 seconds...");
        std::thread::sleep(Duration::from_secs(5));
        running.store(false, Ordering::SeqCst);

        let written = writer.join().unwrap();
        let read = reader.join().unwrap()?;

        let file_size = std::fs::metadata("/tmp/ringlog_stress.log")
            .map(|m| m.len())
            .unwrap_or(0);

        println!("\nResults:");
        println!("  Written to ring: {} events", written);
        println!("  Persisted to disk: {} events", read);
        println!(
            "  Throughput: {:.2}M events/sec",
            written as f64 / 5.0 / 1_000_000.0
        );
        println!("  File size: {:.2} MB", file_size as f64 / 1024.0 / 1024.0);
        
        Ok(())
    })
}
