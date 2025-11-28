# ringlog

A high-performance, zero-copy event logging library for Rust. Built for trading systems and other latency-critical applications.

## Performance

| Test | Throughput | Events |
|------|------------|--------|
| SPSC + Mmap Persistence | 29.96M events/sec | 149.7M in 5s |

## Features

- **Lock-free SPSC Ring Buffer** - Single producer, single consumer with atomic operations
- **Mmap Persistence** - Memory-mapped files for crash recovery and replay
- **Zero-copy Reads** - Direct memory access without allocation
- **Consumer Dispatcher** - Pluggable event consumers with stats tracking

## Architecture
```
┌─────────────────┐     ┌──────────────────┐     ┌─────────────────┐
│ Producer        │────▶│ SPSC Ring Buffer │────▶│ Mmap Log File   │
│ (hot path)      │     │ (lock-free)      │     │ (persistence)   │
└─────────────────┘     └──────────────────┘     └─────────────────┘
```

## Usage

### Ring Buffer
```rust
use ringlog::ring::RingBuffer;
use ringlog::event::EventHeader;

let mut ring = RingBuffer::new(64 * 1024);

let header = EventHeader::new(timestamp, event_type, payload.len() as u16);
ring.write_event(&header, &payload).unwrap();

while let Some((header, payload)) = ring.read_event() {
    // process event
}
```

### SPSC (Multi-threaded)
```rust
use ringlog::ring::SpscRingBuffer;

let ring = SpscRingBuffer::new(64 * 1024 * 1024);
let (mut producer, mut consumer) = ring.split();

// Producer thread
producer.write_event(&header, &payload);

// Consumer thread
while let Some((header, payload)) = consumer.read_event() {
    // process event
}
```

### Mmap Persistence
```rust
use ringlog::storage::{MmapWriter, MmapReader};

// Write
let mut writer = MmapWriter::create("/tmp/events.log", 1024 * 1024 * 1024)?;
writer.write_event(&header, &payload);
writer.sync()?;

// Read (zero-copy replay)
let reader = MmapReader::open("/tmp/events.log")?;
reader.replay(|event| {
    // event.header, event.payload available without copy
});
```

### Event Dispatcher
```rust
use ringlog::consumer::{EventDispatcher, EventConsumer};

struct MyConsumer;

impl EventConsumer for MyConsumer {
    fn consume(&mut self, header: &EventHeader, payload: &[u8]) -> bool {
        // handle event
        true
    }
    fn name(&self) -> &str { "my-consumer" }
}

let mut dispatcher = EventDispatcher::new();
dispatcher.add_consumer(MyConsumer);
dispatcher.drain(&mut ring);
```

## Run
```bash
# Run service
cargo run --release

# Run stress test
cargo run --release --bin stress

# Run tests
cargo test --release
```

## Event Format
```
EventHeader (16 bytes):
┌──────────────┬────────────┬───────┬─────────────┬───────────┐
│ timestamp    │ event_type │ flags │ payload_len │ _reserved │
│ (8 bytes)    │ (1 byte)   │ (1)   │ (2 bytes)   │ (4 bytes) │
└──────────────┴────────────┴───────┴─────────────┴───────────┘

File Format:
┌────────────────────────────────────────┐
│ FileHeader (64 bytes)                  │
│   magic: "EVTL"                        │
│   version: 1                           │
│   event_count, write_offset, etc.      │
├────────────────────────────────────────┤
│ Event 0: [Header][Payload]             │
│ Event 1: [Header][Payload]             │
│ ...                                    │
└────────────────────────────────────────┘
```
