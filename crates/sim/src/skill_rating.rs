pub const SKILL_RATING_BYTES: usize = 96;
const MODES: [&str; 8] = ["tdm", "dm", "dom", "sd", "sab", "ctf", "koth", "dd"];

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SkillRating {
    pub rating: i32,
    pub events: u64,
}

impl Default for SkillRating {
    fn default() -> Self {
        Self {
            rating: 1000,
            events: 0,
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct SkillRatings {
    rows: [SkillRating; 8],
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SkillRatingError {
    InvalidMode,
    InvalidScore,
    InvalidEncoding,
    RatingOverflow,
    EventOverflow,
}

impl SkillRatings {
    pub fn rating(&self, mode: &str) -> Option<SkillRating> {
        mode_index(mode).map(|index| self.rows[index])
    }

    pub fn encode(&self) -> [u8; SKILL_RATING_BYTES] {
        let mut bytes = [0; SKILL_RATING_BYTES];
        for (row, slot) in self.rows.iter().zip(bytes.as_chunks_mut::<12>().0) {
            slot[..4].copy_from_slice(&row.rating.to_le_bytes());
            slot[4..].copy_from_slice(&row.events.to_le_bytes());
        }
        bytes
    }

    pub fn decode(bytes: &[u8]) -> Result<Self, SkillRatingError> {
        if bytes.len() != SKILL_RATING_BYTES {
            return Err(SkillRatingError::InvalidEncoding);
        }
        let mut ratings = Self::default();
        for (row, slot) in ratings.rows.iter_mut().zip(bytes.as_chunks::<12>().0) {
            row.rating = i32::from_le_bytes(
                slot[..4]
                    .try_into()
                    .map_err(|_| SkillRatingError::InvalidEncoding)?,
            );
            row.events = u64::from_le_bytes(
                slot[4..]
                    .try_into()
                    .map_err(|_| SkillRatingError::InvalidEncoding)?,
            );
        }
        Ok(ratings)
    }

    pub(crate) fn updated_pair(
        mut self,
        mut opponent: Self,
        mode: &str,
        score: f32,
    ) -> Result<(Self, Self), SkillRatingError> {
        if !score.is_finite() || !(0.0..=1.0).contains(&score) {
            return Err(SkillRatingError::InvalidScore);
        }
        let index = mode_index(mode).ok_or(SkillRatingError::InvalidMode)?;
        let first = self.rows[index];
        let second = opponent.rows[index];
        let difference = f64::from(second.rating) - f64::from(first.rating);
        let expected = 1.0 / (1.0 + 10.0_f64.powf(difference / 400.0));
        let delta = (32.0 * (f64::from(score) - expected)).round() as i32;
        self.rows[index] = updated(first, delta)?;
        opponent.rows[index] = updated(second, -delta)?;
        Ok((self, opponent))
    }
}

fn mode_index(mode: &str) -> Option<usize> {
    let mode = if mode == "war" { "tdm" } else { mode };
    MODES.iter().position(|candidate| *candidate == mode)
}

fn updated(row: SkillRating, delta: i32) -> Result<SkillRating, SkillRatingError> {
    Ok(SkillRating {
        rating: row
            .rating
            .checked_add(delta)
            .ok_or(SkillRatingError::RatingOverflow)?,
        events: row
            .events
            .checked_add(1)
            .ok_or(SkillRatingError::EventOverflow)?,
    })
}
