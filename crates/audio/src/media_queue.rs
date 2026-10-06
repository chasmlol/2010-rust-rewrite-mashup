use std::collections::VecDeque;
use std::sync::{Condvar, Mutex};
use std::time::Instant;

use crate::clip_store::ClipKey;

const CAPACITY: usize = 256;
const PREWARM_CAPACITY: usize = 192;

pub(crate) struct ClipJob {
    pub key: ClipKey,
    pub queued_at: Instant,
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum MediaPriority {
    Urgent,
    Prewarm,
}

pub(crate) enum QueueRefusal {
    Full,
    Closed,
}

pub(crate) struct MediaJobQueue {
    state: Mutex<QueueState>,
    wake: Condvar,
}

struct QueueState {
    urgent: VecDeque<ClipJob>,
    prewarm: VecDeque<ClipJob>,
    closed: bool,
}

impl MediaJobQueue {
    pub fn new() -> Self {
        Self {
            state: Mutex::new(QueueState {
                urgent: VecDeque::with_capacity(CAPACITY),
                prewarm: VecDeque::with_capacity(PREWARM_CAPACITY),
                closed: false,
            }),
            wake: Condvar::new(),
        }
    }

    pub fn push(&self, job: ClipJob, priority: MediaPriority) -> Result<(), QueueRefusal> {
        let mut state = self
            .state
            .lock()
            .unwrap_or_else(|poison| poison.into_inner());
        if state.closed {
            return Err(QueueRefusal::Closed);
        }
        if state.urgent.len() + state.prewarm.len() == CAPACITY
            || (priority == MediaPriority::Prewarm && state.prewarm.len() == PREWARM_CAPACITY)
        {
            return Err(QueueRefusal::Full);
        }
        match priority {
            MediaPriority::Urgent => state.urgent.push_back(job),
            MediaPriority::Prewarm => state.prewarm.push_back(job),
        }
        self.wake.notify_all();
        Ok(())
    }

    pub fn promote(&self, key: &ClipKey) {
        let mut state = self
            .state
            .lock()
            .unwrap_or_else(|poison| poison.into_inner());
        if state.closed {
            return;
        }
        if let Some(index) = state.prewarm.iter().position(|job| &job.key == key) {
            let job = state.prewarm.remove(index).unwrap();
            state.urgent.push_back(job);
            self.wake.notify_all();
        }
    }

    pub fn pop(&self, urgent_only: bool) -> Option<ClipJob> {
        let mut state = self
            .state
            .lock()
            .unwrap_or_else(|poison| poison.into_inner());
        loop {
            if state.closed {
                return None;
            }
            if let Some(job) = state.urgent.pop_front() {
                return Some(job);
            }
            if !urgent_only && let Some(job) = state.prewarm.pop_front() {
                return Some(job);
            }
            state = self
                .wake
                .wait(state)
                .unwrap_or_else(|poison| poison.into_inner());
        }
    }

    pub fn close(&self) {
        self.state
            .lock()
            .unwrap_or_else(|poison| poison.into_inner())
            .closed = true;
        self.wake.notify_all();
    }
}
