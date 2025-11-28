use crate::event::EventHeader;
pub mod dispatcher;

pub trait EventConsumer: Send {
    fn consume(&mut self, header: &EventHeader, payload: &[u8]) -> bool;

    fn flush(&mut self) {}

    fn name(&self) -> &str;
}
