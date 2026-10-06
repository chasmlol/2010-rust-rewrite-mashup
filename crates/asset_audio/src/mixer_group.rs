#[derive(Clone, Copy, Debug, PartialEq)]
pub struct MixerGroup {
    pub parent: i32,
    pub attenuation: f32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MixerGroupError {
    Cycle { group: usize },
    InvalidParent { group: usize, parent: i32 },
    InvalidAttenuation { group: usize },
    NonFiniteGain { group: usize },
}

pub fn compile_mixer_groups(groups: &[MixerGroup]) -> Vec<Result<f32, MixerGroupError>> {
    let mut gains = vec![None; groups.len()];
    let mut active = vec![None; groups.len()];
    let mut path = Vec::new();
    for first in 0..groups.len() {
        if gains[first].is_some() {
            continue;
        }
        let mut current = first;
        let mut result = loop {
            if let Some(gain) = gains[current] {
                break gain;
            }
            if let Some(position) = active[current] {
                break Err(MixerGroupError::Cycle {
                    group: *path[position..].iter().min().unwrap(),
                });
            }
            active[current] = Some(path.len());
            path.push(current);
            let node = groups[current];
            if !node.attenuation.is_finite() || node.attenuation < 0.0 {
                break Err(MixerGroupError::InvalidAttenuation { group: current });
            }
            if node.parent < 0 {
                break Ok(1.0);
            }
            let parent = node.parent as usize;
            if parent >= groups.len() {
                break Err(MixerGroupError::InvalidParent {
                    group: current,
                    parent: node.parent,
                });
            }
            current = parent;
        };
        while let Some(group) = path.pop() {
            active[group] = None;
            result = result.and_then(|parent_gain| {
                let gain = groups[group].attenuation * parent_gain;
                if gain.is_finite() {
                    Ok(gain)
                } else {
                    Err(MixerGroupError::NonFiniteGain { group })
                }
            });
            gains[group] = Some(result);
        }
    }
    gains.into_iter().map(Option::unwrap).collect()
}
