//! Starfall's preallocated timeline for fixed-size runtime observations.
//!
//! A circular slot array retains chronological entries without per-insert
//! allocation. Count and reported payload size are independent limits.

#[derive(Clone, Debug)]
pub struct HistoryEntry<T> {
    pub tick: u64,
    pub value: T,
    bytes: usize,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum HistoryError {
    Disabled,
    OutOfOrder,
    Oversized,
}

/// Storage is allocated at construction. The byte limit measures caller-reported
/// payload, excluding the preallocated slot array and allocator overhead.
/// Include any nested allocations when reporting a payload's size.
#[derive(Clone, Debug)]
pub struct TickHistory<T> {
    slots: Box<[Option<HistoryEntry<T>>]>,
    oldest: usize,
    count: usize,
    payload_bytes: usize,
    byte_limit: usize,
}

impl<T> TickHistory<T> {
    pub fn new(max_entries: usize, max_bytes: usize) -> Self {
        let capacity = if max_bytes == 0 { 0 } else { max_entries };
        Self {
            slots: std::iter::repeat_with(|| None).take(capacity).collect(),
            oldest: 0,
            count: 0,
            payload_bytes: 0,
            byte_limit: max_bytes,
        }
    }

    /// Reject before mutating the timeline. Tick gaps are legal; restarting
    /// or wrapping the tick counter requires an explicit clear.
    pub fn push(&mut self, tick: u64, value: T, bytes: usize) -> Result<(), HistoryError> {
        if self.slots.is_empty() {
            return Err(HistoryError::Disabled);
        }
        if self
            .iter()
            .next_back()
            .is_some_and(|entry| entry.tick >= tick)
        {
            return Err(HistoryError::OutOfOrder);
        }
        if bytes > self.byte_limit {
            return Err(HistoryError::Oversized);
        }

        let allowed_existing_bytes = self.byte_limit - bytes;
        while self.count == self.slots.len() || self.payload_bytes > allowed_existing_bytes {
            self.discard_oldest();
        }
        let destination = (self.oldest + self.count) % self.slots.len();
        self.slots[destination] = Some(HistoryEntry { tick, value, bytes });
        self.count += 1;
        self.payload_bytes += bytes;
        Ok(())
    }

    fn discard_oldest(&mut self) {
        let entry = self.slots[self.oldest]
            .take()
            .expect("occupied history slot");
        self.payload_bytes -= entry.bytes;
        self.oldest = (self.oldest + 1) % self.slots.len();
        self.count -= 1;
    }

    /// Drop all payloads while retaining allocated slots for the next session.
    pub fn clear(&mut self) {
        while self.count != 0 {
            self.discard_oldest();
        }
        self.oldest = 0;
    }

    pub fn len(&self) -> usize {
        self.count
    }
    pub fn is_empty(&self) -> bool {
        self.count == 0
    }
    pub fn bytes(&self) -> usize {
        self.payload_bytes
    }

    pub fn iter(&self) -> impl DoubleEndedIterator<Item = &HistoryEntry<T>> + ExactSizeIterator {
        (0..self.count).map(|offset| {
            let index = (self.oldest + offset) % self.slots.len();
            self.slots[index].as_ref().expect("occupied history slot")
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn both_limits_evict_oldest_and_rejections_preserve_history() {
        let mut history = TickHistory::new(2, 10);
        history.push(1, "a", 4).unwrap();
        history.push(2, "b", 4).unwrap();
        history.push(3, "c", 7).unwrap();
        assert_eq!(history.iter().map(|e| e.tick).collect::<Vec<_>>(), [3]);
        assert_eq!(
            history.push(3, "duplicate", 1),
            Err(HistoryError::OutOfOrder)
        );
        assert_eq!(
            history.push(4, "oversize", usize::MAX),
            Err(HistoryError::Oversized)
        );
        assert_eq!(history.bytes(), 7);
        history.push(5, "d", 1).unwrap();
        history.push(6, "e", 1).unwrap();
        assert_eq!(history.iter().map(|e| e.tick).collect::<Vec<_>>(), [5, 6]);
        history.clear();
        history.push(0, "new session", 1).unwrap();
        assert_eq!(history.bytes(), 1);
    }

    #[test]
    fn disabled_history_and_overflow_boundary() {
        assert_eq!(
            TickHistory::new(0, 1).push(1, (), 0),
            Err(HistoryError::Disabled)
        );
        assert_eq!(
            TickHistory::new(1, 0).push(1, (), 0),
            Err(HistoryError::Disabled)
        );
        let mut history = TickHistory::new(2, usize::MAX);
        history.push(1, (), usize::MAX).unwrap();
        history.push(2, (), 1).unwrap();
        assert_eq!(history.bytes(), 1);
    }

    #[test]
    fn circular_storage_wraps_in_both_iteration_directions() {
        let mut history = TickHistory::new(3, 12);
        for tick in 0..50 {
            history.push(tick, tick, 4).unwrap();
            let expected: Vec<_> = (tick.saturating_sub(2)..=tick).collect();
            assert_eq!(
                history.iter().map(|entry| entry.value).collect::<Vec<_>>(),
                expected
            );
            assert_eq!(
                history
                    .iter()
                    .rev()
                    .map(|entry| entry.tick)
                    .collect::<Vec<_>>(),
                expected.into_iter().rev().collect::<Vec<_>>()
            );
        }
        history.clear();
        assert_eq!(history.bytes(), 0);
        assert!(history.iter().next_back().is_none());
        history.push(0, 7, 12).unwrap();
        assert_eq!(history.iter().next().unwrap().value, 7);
    }

    #[test]
    fn eviction_and_clear_release_owned_payloads() {
        use crate::budget::ResourceBudget;
        let budget = ResourceBudget::new(3);
        let mut history = TickHistory::new(1, 1);
        history.push(1, budget.reserve(1).unwrap(), 1).unwrap();
        history.push(2, budget.reserve(1).unwrap(), 1).unwrap();
        assert_eq!(budget.snapshot().live, 1);
        history.clear();
        assert_eq!(budget.snapshot().live, 0);
    }
}
