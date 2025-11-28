use super::EventConsumer;
use crate::ring::{Consumer, RingBuffer};

pub struct EventDispatcher {
    consumers: Vec<Box<dyn EventConsumer>>,
}

impl Default for EventDispatcher {
    fn default() -> Self {
        Self::new()
    }
}

impl EventDispatcher {
    pub fn new() -> Self {
        Self {
            consumers: Vec::new(),
        }
    }

    pub fn add_consumer<C: EventConsumer + 'static>(&mut self, consumer: C) {
        self.consumers.push(Box::new(consumer));
    }

    #[inline]
    pub fn drain(&mut self, ring: &mut RingBuffer) -> DrainStats {
        let mut stats = DrainStats::default();
        while let Some((header, payload)) = ring.read_event() {
            stats.events_read += 1;
            for consumer in &mut self.consumers {
                if consumer.consume(&header, &payload) {
                    stats.events_delivered += 1;
                } else {
                    stats.events_failed += 1;
                }
            }
        }
        for consumer in &mut self.consumers {
            consumer.flush();
        }
        stats
    }

    #[inline]
    pub fn drain_spsc(&mut self, consumer: &mut Consumer<'_>) -> DrainStats {
        let mut stats = DrainStats::default();
        while let Some((header, payload)) = consumer.read_event() {
            stats.events_read += 1;
            for c in &mut self.consumers {
                if c.consume(&header, &payload) {
                    stats.events_delivered += 1;
                } else {
                    stats.events_failed += 1;
                }
            }
        }
        for c in &mut self.consumers {
            c.flush();
        }
        stats
    }

    #[inline]
    pub fn drain_batch(&mut self, ring: &mut RingBuffer, limit: usize) -> DrainStats {
        let mut stats = DrainStats::default();
        for _ in 0..limit {
            let Some((header, payload)) = ring.read_event() else {
                break;
            };
            stats.events_read += 1;
            for consumer in &mut self.consumers {
                if consumer.consume(&header, &payload) {
                    stats.events_delivered += 1;
                } else {
                    stats.events_failed += 1;
                }
            }
        }
        stats
    }

    #[inline]
    pub fn drain_spsc_batch(&mut self, consumer: &mut Consumer<'_>, limit: usize) -> DrainStats {
        let mut stats = DrainStats::default();
        for _ in 0..limit {
            let Some((header, payload)) = consumer.read_event() else {
                break;
            };
            stats.events_read += 1;
            for c in &mut self.consumers {
                if c.consume(&header, &payload) {
                    stats.events_delivered += 1;
                } else {
                    stats.events_failed += 1;
                }
            }
        }
        stats
    }
}

#[derive(Debug, Default, Clone, Copy)]
pub struct DrainStats {
    pub events_read: u64,
    pub events_delivered: u64,
    pub events_failed: u64,
}

impl DrainStats {
    #[inline]
    pub fn success_rate(&self) -> f64 {
        let total = self.events_delivered + self.events_failed;
        if total == 0 {
            1.0
        } else {
            self.events_delivered as f64 / total as f64
        }
    }
}
