//! A fixed-capacity queue, after <https://www.ntietz.com/blog/whats-in-a-ring-buffer/>.

use std::collections::VecDeque;

/// A fixed-capacity queue that drops its oldest item when full.
#[derive(Debug)]
pub struct RingBuffer<T> {
    /// The buffered items, oldest first.
    items: VecDeque<T>,
    capacity: usize,
}

impl<T> RingBuffer<T> {
    /// An empty buffer holding at most `capacity` items. Reserves nothing up
    /// front.
    pub fn new(capacity: usize) -> Self {
        RingBuffer {
            items: VecDeque::new(),
            capacity,
        }
    }

    /// Append an item, dropping the oldest if the buffer is already full.
    pub fn push(&mut self, item: T) {
        if self.capacity == 0 {
            return;
        }
        if self.items.len() == self.capacity {
            self.items.pop_front();
        }
        self.items.push_back(item);
    }

    /// How many items are buffered.
    pub fn len(&self) -> usize {
        self.items.len()
    }

    /// Whether nothing is buffered.
    pub fn is_empty(&self) -> bool {
        self.items.is_empty()
    }

    /// The buffered items, oldest first.
    pub fn iter(&self) -> impl Iterator<Item = &T> {
        self.items.iter()
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
    fn a_new_buffer_is_empty() {
        let buffer: RingBuffer<i32> = RingBuffer::new(3);
        assert_eq!(buffer.items.len(), 0);
        assert_eq!(buffer.capacity, 3);
    }

    #[test]
    fn pushing_past_capacity_drops_the_oldest() {
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
    fn pop_takes_the_newest_until_empty() {
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
    fn peek_sees_the_newest_without_taking_it() {
        let mut buffer: RingBuffer<i32> = RingBuffer::new(3);

        buffer.push(1);
        buffer.push(2);
        buffer.push(3);

        assert_eq!(buffer.peek(), Some(&3));

        buffer.push(4);

        assert_eq!(buffer.peek(), Some(&4));
    }

    #[test]
    fn clear_empties_the_buffer() {
        let mut buffer: RingBuffer<i32> = RingBuffer::new(3);

        buffer.push(1);
        buffer.push(2);
        buffer.push(3);

        buffer.clear();

        assert_eq!(buffer.pop(), None);
    }

    #[test]
    fn a_zero_capacity_buffer_holds_nothing() {
        let mut buffer: RingBuffer<i32> = RingBuffer::new(0);
        buffer.push(1);
        assert!(buffer.is_empty());
    }
}
