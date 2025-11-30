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

// Create buffer - returns Result for validation
let mut ring = RingBuffer::new(64 * 1024)?;

let header = EventHeader::new(timestamp, event_type, payload.len() as u16);
ring.write_event(&header, &payload)?;

while let Some((header, payload)) = ring.read_event() {
    // process event
}
```

### SPSC (Multi-threaded)
```rust
use ringlog::ring::SpscRingBuffer;

// Create buffer - returns Result for validation
let ring = SpscRingBuffer::new(64 * 1024 * 1024)?;
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

## Testing

ringlog has comprehensive test coverage across all critical components. All tests use optimized `unwrap()` calls for fast failure detection.

### Test Categories

#### 1. **Ring Buffer Tests** (`ring_buffer` module)
Tests for the core lock-free ring buffer implementation:

- **`new_creates_empty_buffer`** - Verifies buffer initialization
- **`write_single_event`** - Tests basic write operation
- **`read_single_event`** - Tests basic read operation and data integrity
- **`write_multiple_events`** - Validates sequential writes and FIFO ordering
- **`buffer_full_returns_error`** - Tests buffer capacity limits and error handling
- **`wrap_around_works`** - Verifies circular buffer behavior across boundary
- **`capacity_must_be_power_of_two`** - Validates power-of-2 capacity requirement

**What they verify:**
- Zero-copy operations work correctly
- FIFO ordering is maintained
- Circular buffer wrapping is correct
- Error handling for capacity issues

#### 2. **Event Header Tests** (`event_header` module)
Tests for the event header structure:

- **`size_is_16_bytes`** - Ensures fixed 16-byte header size
- **`total_size_includes_payload`** - Validates size calculations
- **`new_sets_fields_correctly`** - Tests header field initialization

**What they verify:**
- Memory layout is correct and aligned
- Size calculations are accurate
- Fields are properly initialized

#### 3. **Event Dispatcher Tests** (`dispatcher` module)
Tests for the consumer dispatch system:

- **`drain_empty_buffer`** - Tests dispatch with no events
- **`drain_delivers_to_consumer`** - Validates event delivery to consumers
- **`drain_tracks_failures`** - Tests failure tracking when consumers fail
- **`drain_batch_respects_limit`** - Verifies batch size limiting
- **`multiple_consumers`** - Tests fanout to multiple consumers
- **`success_rate_calculation`** - Tests statistics calculation
- **`success_rate_empty`** - Edge case for empty stats

**What they verify:**
- Events are correctly delivered to all consumers
- Failure tracking is accurate
- Batch processing works as expected
- Statistics are calculated correctly

#### 4. **Mmap Storage Tests** (`mmap_storage` module)
Tests for memory-mapped file persistence:

- **`create_and_write`** - Tests file creation and writing
- **`write_and_read_back`** - Validates write → read round-trip
- **`iterator_works`** - Tests zero-copy iteration over events
- **`reopen_existing_file`** - Tests file reopening and appending
- **`buffer_full_returns_false`** - Tests capacity limits
- **`invalid_file_returns_error`** - Tests error handling for corrupt files

**What they verify:**
- Data is correctly persisted to disk
- Memory-mapped I/O works reliably
- File format is correct and readable
- Error handling for I/O failures
- Zero-copy replay works

### Running Tests

```bash
# Run all tests
cargo test

# Run tests with output
cargo test -- --nocapture

# Run specific test
cargo test ring_buffer::write_single_event

# Run tests in release mode (faster)
cargo test --release

# Run with all features
cargo test --all-features
```

### Test Philosophy

- **Fast failures**: Tests use `unwrap()` for immediate panic on errors
- **Comprehensive coverage**: All critical paths are tested
- **Isolation**: Each test is independent and can run in parallel
- **Real scenarios**: Tests simulate actual usage patterns
- **Error paths**: Both success and failure cases are tested

### Continuous Testing

```bash
# Watch mode (requires cargo-watch)
cargo watch -x test

# Run tests on every save
cargo watch -x 'test -- --nocapture'
```

## Error Handling

All production code uses `Result` types with detailed error messages. See [ERROR_HANDLING.md](ERROR_HANDLING.md) for details.

**Error types:**
- `RingError::NotEnoughSpace { required, available }` - Buffer capacity exceeded
- `RingError::InvalidCapacity { capacity, reason }` - Invalid buffer size
- `io::Error` with context - File and mmap operation failures

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
