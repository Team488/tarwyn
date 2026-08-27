// see https://www.ntietz.com/blog/whats-in-a-ring-buffer/

use std::collections::VecDeque;

/// A fixed-capacity queue that evicts its oldest item rather than growing.
pub struct RingBuffer<T> {
    /// The buffered items, oldest first.
    pub items: VecDeque<T>,
    capacity: usize,
}

impl<T> RingBuffer<T> {
    /// An empty buffer holding at most `capacity` items.
    pub fn new(capacity: usize) -> Self {
        RingBuffer {
            items: VecDeque::with_capacity(capacity),
            capacity,
        }
    }

    /// Append an item, dropping the oldest if the buffer is already full.
    pub fn push(&mut self, item: T) {
        if self.items.len() == self.capacity {
            self.items.pop_front();
        }
        self.items.push_back(item);
    }

    /// Take the newest item.
    pub fn pop(&mut self) -> Option<T> {
        self.items.pop_back()
    }

    /// Drop every item.
    pub fn clear(&mut self) {
        self.items.clear();
    }

    /// The newest item, without removing it.
    pub fn peek(&self) -> Option<&T> {
        self.items.back()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn creation() {
        let buffer: RingBuffer<i32> = RingBuffer::new(3);
        assert_eq!(buffer.items.len(), 0);
        assert_eq!(buffer.capacity, 3);
    }

    #[test]
    fn push() {
        let mut buffer: RingBuffer<i32> = RingBuffer::new(3);

        buffer.push(1);
        buffer.push(2);
        buffer.push(3);

        assert_eq!(buffer.items.len(), 3);

        buffer.push(4);

        assert_eq!(buffer.items.len(), 3);
        assert_eq!(buffer.items[2], 4);
    }

    #[test]
    fn pop() {
        let mut buffer: RingBuffer<i32> = RingBuffer::new(3);

        buffer.push(1);
        buffer.push(2);
        buffer.push(3);

        assert_eq!(buffer.pop(), Some(3));

        buffer.push(4);

        assert_eq!(buffer.pop(), Some(4));

        buffer.pop();
        buffer.pop();
        buffer.pop();
        assert_eq!(buffer.pop(), None);
    }

    #[test]
    fn peek() {
        let mut buffer: RingBuffer<i32> = RingBuffer::new(3);

        buffer.push(1);
        buffer.push(2);
        buffer.push(3);

        assert_eq!(buffer.peek(), Some(&3));

        buffer.push(4);

        assert_eq!(buffer.peek(), Some(&4));
    }

    #[test]
    fn clear() {
        let mut buffer: RingBuffer<i32> = RingBuffer::new(3);

        buffer.push(1);
        buffer.push(2);
        buffer.push(3);

        buffer.clear();

        assert_eq!(buffer.pop(), None);
    }
}
