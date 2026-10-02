//! Shared capacity accounting for Starfall runtime services.
//!
//! A mutex protects one ledger, making admission and diagnostic snapshots
//! coherent. A reservation can move into a deferred ECS command; cancelling
//! that command or destroying its entity returns capacity automatically.

use std::sync::{Arc, Mutex, MutexGuard};

#[derive(Debug)]
struct CapacityLedger {
    limit: usize,
    available: usize,
    peak: usize,
    refused: u64,
}

fn ledger_lock(ledger: &Mutex<CapacityLedger>) -> MutexGuard<'_, CapacityLedger> {
    ledger
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
}

/// Handles cloned from one budget share capacity. New budgets are independent.
#[derive(Clone, Debug)]
pub struct ResourceBudget {
    ledger: Arc<Mutex<CapacityLedger>>,
}

/// A coherent view of the ledger at the instant it was locked.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct BudgetSnapshot {
    pub limit: usize,
    pub live: usize,
    pub peak: usize,
    pub refused: u64,
}

impl ResourceBudget {
    pub fn new(limit: usize) -> Self {
        Self {
            ledger: Arc::new(Mutex::new(CapacityLedger {
                limit,
                available: limit,
                peak: 0,
                refused: 0,
            })),
        }
    }

    /// Admission never waits for capacity. It only locks the accounting ledger.
    /// A zero-unit reservation succeeds; a refused request changes no capacity.
    pub fn reserve(&self, units: usize) -> Option<Reservation> {
        let mut ledger = ledger_lock(&self.ledger);
        let Some(remaining) = ledger.available.checked_sub(units) else {
            ledger.refused = ledger.refused.saturating_add(1);
            return None;
        };
        ledger.available = remaining;
        ledger.peak = ledger.peak.max(ledger.limit - remaining);
        Some(Reservation {
            ledger: Arc::clone(&self.ledger),
            units,
        })
    }

    pub fn snapshot(&self) -> BudgetSnapshot {
        let ledger = ledger_lock(&self.ledger);
        BudgetSnapshot {
            limit: ledger.limit,
            live: ledger.limit - ledger.available,
            peak: ledger.peak,
            refused: ledger.refused,
        }
    }
}

/// A uniquely owned claim on capacity; move it into the resource being tracked.
#[derive(Debug)]
#[must_use = "keep the reservation alive for as long as its resource exists"]
pub struct Reservation {
    ledger: Arc<Mutex<CapacityLedger>>,
    units: usize,
}

impl Drop for Reservation {
    fn drop(&mut self) {
        // Each non-cloneable ticket restores exactly what admission deducted.
        ledger_lock(&self.ledger).available += self.units;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn releases_and_rejects_without_leaking_or_overflowing() {
        let budget = ResourceBudget::new(4);
        let reservation = budget.reserve(4).unwrap();
        assert!(budget.reserve(1).is_none());
        assert!(budget.reserve(usize::MAX).is_none());
        assert!(budget.reserve(0).is_some());
        assert_eq!(budget.snapshot().live, 4);
        drop(reservation);
        assert_eq!(
            budget.snapshot(),
            BudgetSnapshot {
                limit: 4,
                live: 0,
                peak: 4,
                refused: 2
            }
        );
        assert!(budget.reserve(4).is_some());
    }

    #[test]
    fn concurrent_reservations_cannot_overbook() {
        let budget = ResourceBudget::new(3);
        let barrier = Arc::new(std::sync::Barrier::new(12));
        std::thread::scope(|scope| {
            for _ in 0..12 {
                let budget = budget.clone();
                let barrier = barrier.clone();
                scope.spawn(move || {
                    let reservation = budget.reserve(1);
                    barrier.wait();
                    assert_eq!(budget.snapshot().live, 3);
                    barrier.wait();
                    drop(reservation);
                });
            }
        });
        assert_eq!(
            budget.snapshot(),
            BudgetSnapshot {
                limit: 3,
                live: 0,
                peak: 3,
                refused: 9
            }
        );
    }

    #[test]
    fn tickets_outlive_handles_and_separate_budgets_do_not_share_capacity() {
        let first = ResourceBudget::new(1);
        let observer = first.clone();
        let other = ResourceBudget::new(1);
        let ticket = first.reserve(1).unwrap();
        drop(first);
        assert_eq!(observer.snapshot().live, 1);
        let other_ticket = other.reserve(1).unwrap();
        drop(ticket);
        assert_eq!(observer.snapshot().live, 0);
        assert_eq!(other.snapshot().live, 1);
        drop(other_ticket);
    }
}
