use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

#[derive(Clone, Default)]
pub(crate) struct PendingBudget(Arc<AtomicUsize>);

struct Reservation(Arc<AtomicUsize>);

#[derive(Clone)]
pub(crate) struct PendingTicket {
    _reservation: Arc<Reservation>,
}

impl PendingBudget {
    pub fn reserve(&self) -> Option<PendingTicket> {
        self.0
            .fetch_update(Ordering::AcqRel, Ordering::Acquire, |count| {
                (count < crate::runtime::LOGICAL_INSTANCES).then_some(count + 1)
            })
            .ok()?;
        Some(PendingTicket {
            _reservation: Arc::new(Reservation(self.0.clone())),
        })
    }
}

impl Drop for Reservation {
    fn drop(&mut self) {
        self.0.fetch_sub(1, Ordering::AcqRel);
    }
}
