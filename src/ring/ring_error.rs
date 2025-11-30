use std::fmt;

#[derive(Debug)]
pub enum RingError {
    NotEnoughSpace {
        required: usize,
        available: usize,
    },
    InvalidCapacity {
        capacity: usize,
        reason: &'static str,
    },
    PayloadTooLarge {
        payload_len: usize,
        max_len: usize,
    },
}

impl fmt::Display for RingError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NotEnoughSpace { required, available } => {
                write!(
                    f,
                    "Not enough space in ring buffer: required {} bytes, available {} bytes",
                    required, available
                )
            }
            Self::InvalidCapacity { capacity, reason } => {
                write!(f, "Invalid capacity {}: {}", capacity, reason)
            }
            Self::PayloadTooLarge { payload_len, max_len } => {
                write!(
                    f,
                    "Payload too large: {} bytes exceeds maximum of {} bytes",
                    payload_len, max_len
                )
            }
        }
    }
}

impl std::error::Error for RingError {}
