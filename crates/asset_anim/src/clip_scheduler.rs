use std::cmp::Ordering;
use std::collections::BinaryHeap;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering as AtomicOrdering};

static NEXT_SCHEDULER: AtomicU64 = AtomicU64::new(1);
const NOTIFY_CAPACITY: usize = 4096;

use crate::xanim_clip::AnimClip;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ClipSchedulerError {
    BadNode { node: usize, len: usize },
}

impl core::fmt::Display for ClipSchedulerError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Self::BadNode { node, len } => {
                write!(
                    f,
                    "clip scheduler node {node} is out of range (has {len} nodes)"
                )
            }
        }
    }
}

impl std::error::Error for ClipSchedulerError {}

#[derive(Debug, Clone, Copy)]
pub struct ActiveAnim<'a> {
    pub node: usize,
    pub clip: &'a AnimClip,
    pub time: f32,
    pub rate: f32,
    pub weight: f32,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ClipNotification {
    pub scheduler: u64,
    pub playback: u64,
    pub node: usize,
    pub cycle: i64,
    pub marker: usize,
    pub name: String,
}

#[derive(Debug, Default)]
pub struct ClipAdvance {
    pub notifications: Vec<ClipNotification>,
    pub discarded: u64,
}

#[derive(Debug)]
struct Crossing {
    offset: f64,
    spacing: f64,
    node: usize,
    marker: usize,
    cycle: i64,
    step: i64,
    remaining: u64,
}

impl PartialEq for Crossing {
    fn eq(&self, other: &Self) -> bool {
        self.cmp(other) == Ordering::Equal
    }
}
impl Eq for Crossing {}
impl PartialOrd for Crossing {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}
impl Ord for Crossing {
    fn cmp(&self, other: &Self) -> Ordering {
        other
            .offset
            .total_cmp(&self.offset)
            .then_with(|| other.node.cmp(&self.node))
            .then_with(|| other.marker.cmp(&self.marker))
    }
}

#[derive(Debug, Default)]
struct Node {
    clip: Option<Arc<AnimClip>>,
    time: f64,
    cycle: i64,
    playback: u64,
    initial: bool,
    rate: f32,
    weight: f32,
    goal_weight: f32,
    goal_time_remaining: f32,
}

#[derive(Debug)]
pub struct ClipScheduler {
    identity: u64,
    next_playback: u64,
    nodes: Vec<Node>,
}

impl ClipScheduler {
    pub fn new(node_count: usize) -> Self {
        Self {
            identity: NEXT_SCHEDULER
                .fetch_update(AtomicOrdering::Relaxed, AtomicOrdering::Relaxed, |value| {
                    value.checked_add(1)
                })
                .expect("clip scheduler identity exhausted"),
            next_playback: 1,
            nodes: (0..node_count)
                .map(|_| Node {
                    rate: 1.0,
                    ..Default::default()
                })
                .collect(),
        }
    }

    pub fn node_count(&self) -> usize {
        self.nodes.len()
    }

    pub fn set_clip(&mut self, node: usize, clip: Arc<AnimClip>) -> Result<(), ClipSchedulerError> {
        self.node(node)?;
        let playback = self.next_playback;
        self.next_playback = self
            .next_playback
            .checked_add(1)
            .expect("clip playback identity exhausted");
        let node = self.node_mut(node)?;
        node.clip = Some(clip);
        node.time = 0.0;
        node.cycle = 0;
        node.playback = playback;
        node.initial = true;
        Ok(())
    }

    pub fn clear(&mut self, node: usize) -> Result<(), ClipSchedulerError> {
        *self.node_mut(node)? = Node {
            rate: 1.0,
            ..Default::default()
        };
        Ok(())
    }

    pub fn set_time(&mut self, node: usize, time: f32) -> Result<(), ClipSchedulerError> {
        let node = self.node_mut(node)?;
        node.time = if time.is_finite() {
            f64::from(time.max(0.0))
        } else {
            0.0
        };
        node.initial = false;
        Ok(())
    }

    pub fn set_rate(&mut self, node: usize, rate: f32) -> Result<(), ClipSchedulerError> {
        self.node_mut(node)?.rate = if rate.is_finite() { rate } else { 0.0 };
        Ok(())
    }

    pub fn set_goal_weight(
        &mut self,
        node: usize,
        goal_weight: f32,
        goal_time: f32,
    ) -> Result<(), ClipSchedulerError> {
        let node = self.node_mut(node)?;
        node.goal_weight = goal_weight.max(0.0);
        node.goal_time_remaining = goal_time.max(0.0);
        if node.goal_time_remaining == 0.0 {
            node.weight = node.goal_weight;
        }
        Ok(())
    }

    pub fn advance(&mut self, dt: f32) -> ClipAdvance {
        let dt = if dt.is_finite() { dt.max(0.0) } else { 0.0 };
        let mut crossings = BinaryHeap::new();
        let mut total = 0u64;
        for (index, node) in self.nodes.iter_mut().enumerate() {
            if node.weight > 0.0 {
                collect_crossings(node, index, dt, &mut crossings, &mut total);
            }
            advance_goal(node, dt);
        }
        let mut notifications = Vec::with_capacity((total.min(NOTIFY_CAPACITY as u64)) as usize);
        while notifications.len() < NOTIFY_CAPACITY {
            let Some(mut crossing) = crossings.pop() else {
                break;
            };
            let node = &self.nodes[crossing.node];
            let clip = node.clip.as_ref().expect("active clip");
            notifications.push(ClipNotification {
                scheduler: self.identity,
                playback: node.playback,
                node: crossing.node,
                cycle: crossing.cycle,
                marker: crossing.marker,
                name: clip.notifies[crossing.marker].name.clone(),
            });
            crossing.remaining -= 1;
            if crossing.remaining != 0 {
                crossing.cycle = crossing.cycle.saturating_add(crossing.step);
                crossing.offset += crossing.spacing;
                crossings.push(crossing);
            }
        }
        ClipAdvance {
            discarded: total.saturating_sub(notifications.len() as u64),
            notifications,
        }
    }

    pub fn active(&self) -> impl Iterator<Item = ActiveAnim<'_>> {
        self.nodes.iter().enumerate().filter_map(|(node, state)| {
            (state.weight > 0.0).then(|| {
                state.clip.as_deref().map(|clip| ActiveAnim {
                    node,
                    clip,
                    time: state.time as f32,
                    rate: state.rate,
                    weight: state.weight,
                })
            })?
        })
    }

    pub fn weight(&self, node: usize) -> Result<f32, ClipSchedulerError> {
        Ok(self.node(node)?.weight)
    }

    pub fn time(&self, node: usize) -> Result<f32, ClipSchedulerError> {
        Ok(self.node(node)?.time as f32)
    }

    pub fn rate(&self, node: usize) -> Result<f32, ClipSchedulerError> {
        Ok(self.node(node)?.rate)
    }

    fn node(&self, node: usize) -> Result<&Node, ClipSchedulerError> {
        self.nodes.get(node).ok_or(ClipSchedulerError::BadNode {
            node,
            len: self.nodes.len(),
        })
    }

    fn node_mut(&mut self, node: usize) -> Result<&mut Node, ClipSchedulerError> {
        let len = self.nodes.len();
        self.nodes
            .get_mut(node)
            .ok_or(ClipSchedulerError::BadNode { node, len })
    }
}

fn collect_crossings(
    node: &mut Node,
    index: usize,
    dt: f32,
    crossings: &mut BinaryHeap<Crossing>,
    total: &mut u64,
) {
    let Some(clip) = &node.clip else { return };
    let duration = f64::from(clip.duration());
    if !duration.is_finite() || duration <= f64::from(f32::EPSILON) {
        node.time = 0.0;
        return;
    }
    let rate = f64::from(node.rate);
    let old = if clip.looping {
        node.cycle as f64 + node.time / duration
    } else {
        node.time / duration
    };
    let next = old + f64::from(dt) * rate / duration;
    let new = if clip.looping {
        next
    } else {
        next.clamp(0.0, 1.0)
    };
    let initial = node.initial;
    if new != old {
        node.initial = false;
    }
    if clip.looping {
        node.cycle = new.floor() as i64;
        node.time = new.rem_euclid(1.0) * duration;
    } else {
        node.time = new * duration;
    }
    if node.goal_weight <= 0.0 || new == old {
        return;
    }
    for (marker, notify) in clip.notifies.iter().enumerate() {
        if notify.name.is_empty()
            || notify.name.eq_ignore_ascii_case("end")
            || !notify.time.is_finite()
        {
            continue;
        }
        let point = f64::from(notify.time.clamp(0.0, 1.0));
        let (first, last, step) = if new > old {
            let first = if initial && old == 0.0 && point == 0.0 {
                0
            } else {
                ((old - point).floor() as i64).saturating_add(1)
            };
            (first, (new - point).floor() as i64, 1)
        } else {
            (
                ((old - point).ceil() as i64).saturating_sub(1),
                (new - point).ceil() as i64,
                -1,
            )
        };
        if !clip.looping
            && ((step == 1 && (first > 0 || last < 0)) || (step == -1 && (first < 0 || last > 0)))
        {
            continue;
        }
        let (first, last) = if clip.looping { (first, last) } else { (0, 0) };
        if (step == 1 && first > last) || (step == -1 && first < last) {
            continue;
        }
        let count = first.abs_diff(last).saturating_add(1);
        *total = total.saturating_add(count);
        crossings.push(Crossing {
            offset: (first as f64 + point - old) * duration / rate,
            spacing: duration / rate.abs(),
            node: index,
            marker,
            cycle: first,
            step,
            remaining: count,
        });
    }
}

fn advance_goal(node: &mut Node, dt: f32) {
    if node.goal_time_remaining <= 0.0 {
        return;
    }
    let step = dt.min(node.goal_time_remaining);
    node.weight += (node.goal_weight - node.weight) * step / node.goal_time_remaining;
    node.goal_time_remaining -= step;
    if node.goal_time_remaining <= f32::EPSILON {
        node.weight = node.goal_weight;
        node.goal_time_remaining = 0.0;
    }
}
