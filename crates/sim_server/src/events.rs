use crate::types::{EventCursor, ServerEvent};
use sim_core::Tick;

/// A ring buffer for storing events with cursor-based retrieval.
pub struct EventBuffer<E> {
    buffer: Vec<Option<ServerEvent<E>>>,
    capacity: usize,
    next_sequence: u64,
}

impl<E: Clone> EventBuffer<E> {
    /// Create a new event buffer with the given capacity.
    pub fn new(capacity: usize) -> Self {
        let capacity = capacity.max(1);
        Self {
            buffer: (0..capacity).map(|_| None).collect(),
            capacity,
            next_sequence: 0,
        }
    }

    /// Push a new event into the buffer.
    pub fn push(&mut self, tick: Tick, event: E) {
        let sequence = self.next_sequence;
        self.next_sequence += 1;

        let index = (sequence as usize) % self.capacity;
        self.buffer[index] = Some(ServerEvent {
            sequence,
            tick,
            event,
        });
    }

    /// Get events starting from the given cursor.
    /// Returns the events and a new cursor pointing past the last returned event.
    pub fn get_from_cursor(&self, cursor: EventCursor) -> (Vec<ServerEvent<E>>, EventCursor) {
        let mut events = Vec::new();
        let start_seq = cursor.0;

        // If no events yet, return empty
        if self.next_sequence == 0 {
            return (events, EventCursor(0));
        }

        // Calculate the oldest available sequence
        let oldest_available = if self.next_sequence > self.capacity as u64 {
            self.next_sequence - self.capacity as u64
        } else {
            0
        };

        // Start from the requested cursor or oldest available, whichever is newer
        let effective_start = start_seq.max(oldest_available);

        // Collect all events from effective_start to next_sequence
        for seq in effective_start..self.next_sequence {
            let index = (seq as usize) % self.capacity;
            if let Some(event) = &self.buffer[index] {
                if event.sequence == seq {
                    events.push(event.clone());
                }
            }
        }

        (events, EventCursor(self.next_sequence))
    }

    /// Get the current sequence number (next cursor position).
    pub fn current_sequence(&self) -> u64 {
        self.next_sequence
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_push_and_retrieve() {
        let mut buffer: EventBuffer<i32> = EventBuffer::new(10);

        buffer.push(1, 100);
        buffer.push(2, 200);
        buffer.push(3, 300);

        let (events, cursor) = buffer.get_from_cursor(EventCursor(0));
        assert_eq!(events.len(), 3);
        assert_eq!(events[0].sequence, 0);
        assert_eq!(events[0].tick, 1);
        assert_eq!(events[0].event, 100);
        assert_eq!(events[1].sequence, 1);
        assert_eq!(events[2].sequence, 2);
        assert_eq!(cursor.0, 3);
    }

    #[test]
    fn test_cursor_continuation() {
        let mut buffer: EventBuffer<i32> = EventBuffer::new(10);

        buffer.push(1, 100);
        buffer.push(2, 200);

        let (events, cursor) = buffer.get_from_cursor(EventCursor(0));
        assert_eq!(events.len(), 2);

        buffer.push(3, 300);
        buffer.push(4, 400);

        let (events, cursor) = buffer.get_from_cursor(cursor);
        assert_eq!(events.len(), 2);
        assert_eq!(events[0].sequence, 2);
        assert_eq!(events[1].sequence, 3);
        assert_eq!(cursor.0, 4);
    }

    #[test]
    fn test_overflow_drops_old_events() {
        let mut buffer: EventBuffer<i32> = EventBuffer::new(3);

        buffer.push(1, 100);
        buffer.push(2, 200);
        buffer.push(3, 300);
        buffer.push(4, 400); // This should overwrite event 100

        let (events, cursor) = buffer.get_from_cursor(EventCursor(0));
        assert_eq!(events.len(), 3);
        assert_eq!(events[0].sequence, 1); // Oldest available
        assert_eq!(events[0].event, 200);
        assert_eq!(cursor.0, 4);
    }

    #[test]
    fn test_cursor_past_available() {
        let mut buffer: EventBuffer<i32> = EventBuffer::new(3);

        for i in 0..10 {
            buffer.push(i, i as i32 * 100);
        }

        // Request from cursor 0 but oldest available is 7
        let (events, cursor) = buffer.get_from_cursor(EventCursor(0));
        assert_eq!(events.len(), 3);
        assert_eq!(events[0].sequence, 7);
        assert_eq!(cursor.0, 10);
    }

    #[test]
    fn test_empty_buffer() {
        let buffer: EventBuffer<i32> = EventBuffer::new(10);
        let (events, cursor) = buffer.get_from_cursor(EventCursor(0));
        assert!(events.is_empty());
        assert_eq!(cursor.0, 0);
    }

    #[test]
    fn test_cursor_at_end() {
        let mut buffer: EventBuffer<i32> = EventBuffer::new(10);

        buffer.push(1, 100);
        buffer.push(2, 200);

        let (events, _) = buffer.get_from_cursor(EventCursor(2));
        assert!(events.is_empty());
    }
}
