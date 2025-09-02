pub mod utils {
    pub mod ports;
    pub mod ring_buffer;
}

pub mod tarwyn_client;
pub mod tarwyn_server;

pub mod tarwyn {
    include!(concat!(env!("OUT_DIR"), "/tarwyn.rs"));
}

#[cfg(test)]
mod tests {
    use crate::utils::ring_buffer::RingBuffer;

    #[test]
    fn test_ring_buffer_new() {
        let buffer: RingBuffer<i32> = RingBuffer::new(3);
        assert_eq!(buffer.items.len(), 0);
    }

    #[test]
    fn test_ring_buffer_push_within_capacity() {
        let mut buffer = RingBuffer::new(3);
        buffer.push(1);
        buffer.push(2);
        buffer.push(3);
        assert_eq!(buffer.items.len(), 3);
        assert_eq!(buffer.items[0], 1);
        assert_eq!(buffer.items[1], 2);
        assert_eq!(buffer.items[2], 3);
    }

    #[test]
    fn test_ring_buffer_push_exceeding_capacity() {
        let mut buffer = RingBuffer::new(3);
        buffer.push(1);
        buffer.push(2);
        buffer.push(3);
        buffer.push(4); // This should remove the first element (1)
        assert_eq!(buffer.items.len(), 3);
        assert_eq!(buffer.items[0], 2);
        assert_eq!(buffer.items[1], 3);
        assert_eq!(buffer.items[2], 4);
    }

    #[test]
    fn test_ring_buffer_pop() {
        let mut buffer = RingBuffer::new(3);
        buffer.push(1);
        buffer.push(2);
        buffer.push(3);
        assert_eq!(buffer.pop(), Some(1));
        assert_eq!(buffer.pop(), Some(2));
        assert_eq!(buffer.pop(), Some(3));
        assert_eq!(buffer.pop(), None); // Buffer is empty
    }

    #[test]
    fn test_ring_buffer_empty_pop() {
        let mut buffer: RingBuffer<i32> = RingBuffer::new(3);
        assert_eq!(buffer.pop(), None); // Popping from an empty buffer
    }
}
