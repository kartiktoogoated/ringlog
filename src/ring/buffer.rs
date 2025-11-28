pub struct RingBuffer {
    pub buf: Vec<u8>,
    pub capacity: usize,
    pub head: usize,
    pub tail: usize,
}
