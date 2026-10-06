use sim::Snapshot;
use std::collections::BTreeMap;
use std::sync::Arc;

use crate::transport::frame::FrameParts;

#[derive(Debug)]
struct SentFrame {
    snapshot: Arc<Snapshot>,
    parts: FrameParts,
}

#[derive(Debug, Default)]
pub struct AckedBaselineTable {
    sent: BTreeMap<u32, SentFrame>,

    highest_acked: Option<u32>,
    max_retained: usize,
}

impl AckedBaselineTable {
    pub fn new(max_retained: usize) -> Self {
        Self {
            sent: BTreeMap::new(),
            highest_acked: None,
            max_retained: max_retained.max(1),
        }
    }

    pub fn remember(&mut self, snapshot_seq: u32, snapshot: Arc<Snapshot>, parts: FrameParts) {
        self.sent
            .insert(snapshot_seq, SentFrame { snapshot, parts });
        self.trim();
    }

    pub fn ack(&mut self, snapshot_seq: u32) -> Option<sim::Tick> {
        let tick = self.sent.get(&snapshot_seq)?.snapshot.tick;
        self.highest_acked = Some(match self.highest_acked {
            Some(prev) => prev.max(snapshot_seq),
            None => snapshot_seq,
        });

        if let Some(acked) = self.highest_acked {
            self.sent.retain(|seq, _| *seq >= acked);
        }
        Some(tick)
    }

    pub fn baseline_seq_for_encode(&self) -> u32 {
        match self.highest_acked {
            Some(seq) if self.sent.contains_key(&seq) => seq,
            _ => 0,
        }
    }

    pub fn clear_missing_ack(&mut self) -> bool {
        match self.highest_acked {
            Some(seq) if !self.sent.contains_key(&seq) => {
                self.highest_acked = None;
                true
            }
            _ => false,
        }
    }

    fn is_pinned(&self, seq: u32) -> bool {
        if self.highest_acked == Some(seq) {
            return true;
        }
        let newest = self.sent.keys().next_back().copied();
        if newest == Some(seq) {
            return true;
        }
        self.highest_acked.is_none() && self.sent.keys().next().copied() == Some(seq)
    }

    fn trim(&mut self) {
        while self.sent.len() > self.max_retained {
            let victim = self.sent.keys().copied().find(|seq| !self.is_pinned(*seq));
            match victim {
                Some(seq) => {
                    self.sent.remove(&seq);
                }
                None => break,
            }
        }
    }

    pub fn highest_acked(&self) -> Option<u32> {
        self.highest_acked
    }

    pub fn baseline_for_encode(&self) -> Option<&Snapshot> {
        let acked = self.highest_acked?;
        self.sent.get(&acked).map(|sent| sent.snapshot.as_ref())
    }

    pub fn baseline_parts_for_encode(&self) -> Option<&FrameParts> {
        let acked = self.highest_acked?;
        self.sent.get(&acked).map(|sent| &sent.parts)
    }

    pub fn may_encode_against(&self, baseline_seq: u32) -> bool {
        if baseline_seq == 0 {
            return true;
        }
        self.highest_acked == Some(baseline_seq) && self.sent.contains_key(&baseline_seq)
    }
}

#[derive(Debug, Default)]
pub struct FlakyDatagramQueue {
    pending: Vec<(u32, Vec<u8>)>,
    next_tag: u32,
}

impl FlakyDatagramQueue {
    pub fn push(&mut self, bytes: Vec<u8>) -> u32 {
        let tag = self.next_tag;
        self.next_tag = self.next_tag.wrapping_add(1);
        self.pending.push((tag, bytes));
        tag
    }

    pub fn len(&self) -> usize {
        self.pending.len()
    }
}
