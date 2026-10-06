use crate::penetration::{Contact, Vec3, vec_cmp};
pub const MAX_CONTACTS: usize = 32;
const UNIT_TOL: f64 = 1e-8;
const FEASIBILITY_TOL: f64 = 2e-10;
const DEPENDENCE_TOL: f64 = 1e-12;

#[derive(Clone, Copy, Debug)]
pub struct Config {
    pub max_displacement: f64,
    pub margin: f64,
    pub iterations: u32,
    pub capacity: usize,
}
impl Default for Config {
    fn default() -> Self {
        Self {
            max_displacement: 8.0,
            margin: 0.02,
            iterations: 8,
            capacity: 32,
        }
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Coverage {
    Complete,
    Unsupported,
    Overflow,
}
#[derive(Clone, Debug)]
pub struct ContactBuffer {
    data: [Contact; MAX_CONTACTS],
    len: usize,
    capacity: usize,
    overflowed: bool,
}
impl Default for ContactBuffer {
    fn default() -> Self {
        Self::new()
    }
}
impl ContactBuffer {
    pub fn new() -> Self {
        Self::with_capacity(MAX_CONTACTS)
    }
    pub fn with_capacity(capacity: usize) -> Self {
        Self {
            data: [Contact::default(); MAX_CONTACTS],
            len: 0,
            capacity: capacity.min(MAX_CONTACTS),
            overflowed: capacity > MAX_CONTACTS,
        }
    }
    pub fn push(&mut self, c: Contact) -> bool {
        if self.len >= self.capacity {
            self.overflowed = true;
            return false;
        }
        self.data[self.len] = c;
        self.len += 1;
        true
    }
    pub fn as_slice(&self) -> &[Contact] {
        &self.data[..self.len]
    }
    pub fn len(&self) -> usize {
        self.len
    }
    pub fn is_empty(&self) -> bool {
        self.len == 0
    }
    pub fn overflowed(&self) -> bool {
        self.overflowed
    }
    pub fn capacity(&self) -> usize {
        self.capacity
    }
}
pub trait Backend {
    fn contacts(&mut self, position: Vec3, out: &mut ContactBuffer) -> Coverage;
    fn clear(&mut self, position: Vec3) -> bool;
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SolveError {
    InvalidInput,
    Capacity,
    Infeasible,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Status {
    Success,
    InvalidInput,
    Unsupported,
    Overflow,
    Infeasible,
    NoProgress,
    BudgetExceeded,
    IterationLimit,
    FinalRejected,
}
#[derive(Clone, Copy, Debug)]
pub struct Outcome {
    pub position: Vec3,
    pub status: Status,
    pub queries: u32,
    pub corrections: u32,
}
impl Outcome {
    pub fn success(&self) -> bool {
        self.status == Status::Success
    }
}
fn contact_cmp(a: Contact, b: Contact) -> core::cmp::Ordering {
    vec_cmp(a.normal, b.normal).then(a.depth.total_cmp(&b.depth))
}
fn valid_contact(c: Contact) -> bool {
    c.normal.finite()
        && c.depth.is_finite()
        && c.depth >= 0.0
        && (c.normal.length_squared() - 1.0).abs() <= UNIT_TOL
}
pub fn minimum_correction(contacts: &[Contact], margin: f64) -> Result<Vec3, SolveError> {
    if !margin.is_finite() || margin < 0.0 {
        return Err(SolveError::InvalidInput);
    }
    if contacts.len() > MAX_CONTACTS {
        return Err(SolveError::Capacity);
    }
    let mut sorted = [Contact::default(); MAX_CONTACTS];
    for (i, &c) in contacts.iter().enumerate() {
        if !valid_contact(c) || (c.depth + margin).is_infinite() {
            return Err(SolveError::InvalidInput);
        }
        sorted[i] = c;
    }
    for i in 1..contacts.len() {
        let mut j = i;
        while j > 0 && contact_cmp(sorted[j], sorted[j - 1]).is_lt() {
            sorted.swap(j, j - 1);
            j -= 1;
        }
    }
    let cs = &sorted[..contacts.len()];
    let mut best: Option<Vec3> = None;
    let mut consider = |mut v: Vec3| {
        if !v.finite() {
            return;
        }
        let norm = v.length();
        if !norm.is_finite() {
            return;
        }
        let mut scale = 1.0f64;
        for c in cs {
            let rhs = c.depth + margin;
            let value = c.normal.dot(v);
            if !value.is_finite() || value < rhs - FEASIBILITY_TOL * (1.0 + rhs + norm) {
                return;
            }
            if value < rhs {
                if value <= 0.0 {
                    return;
                }
                scale = scale.max(rhs / value);
            }
        }
        if scale > 1.0 {
            v = v * (scale * (1.0 + 4.0 * f64::EPSILON));
        }
        let norm2 = v.length_squared();
        if !norm2.is_finite() {
            return;
        }
        for c in cs {
            let rhs = c.depth + margin;
            if c.normal.dot(v) < rhs - FEASIBILITY_TOL * (1.0 + rhs + libm::sqrt(norm2)) {
                return;
            }
        }
        if best.is_none_or(|b| {
            norm2
                .total_cmp(&b.length_squared())
                .then(vec_cmp(v, b))
                .is_lt()
        }) {
            best = Some(v);
        }
    };
    consider(Vec3::ZERO);
    for i in 0..cs.len() {
        let a = cs[i].normal;
        let ra = cs[i].depth + margin;
        consider(a * (ra / a.length_squared()));
        for j in i + 1..cs.len() {
            let b = cs[j].normal;
            let rb = cs[j].depth + margin;
            let c = a.cross(b);
            let cc = c.length_squared();
            if cc > DEPENDENCE_TOL * DEPENDENCE_TOL {
                consider((b.cross(c) * ra + c.cross(a) * rb) / cc);
            }
            for contact in cs.iter().skip(j + 1) {
                let d = contact.normal;
                let rd = contact.depth + margin;
                let det = a.dot(b.cross(d));
                if det.abs() > DEPENDENCE_TOL {
                    consider((b.cross(d) * ra + d.cross(a) * rb + a.cross(b) * rd) / det);
                }
            }
        }
    }
    best.ok_or(SolveError::Infeasible)
}

pub fn recover<B: Backend>(origin: Vec3, config: Config, backend: &mut B) -> Outcome {
    let mut out = Outcome {
        position: origin,
        status: Status::InvalidInput,
        queries: 0,
        corrections: 0,
    };
    if !origin.finite()
        || !config.max_displacement.is_finite()
        || config.max_displacement <= 0.0
        || !config.margin.is_finite()
        || config.margin < 0.0
        || config.iterations == 0
        || config.iterations > 1024
        || config.capacity == 0
        || config.capacity > MAX_CONTACTS
    {
        return out;
    }
    let mut candidate = origin;
    for iteration in 0..=config.iterations {
        let mut buffer = ContactBuffer::with_capacity(config.capacity);
        let coverage = backend.contacts(candidate, &mut buffer);
        out.queries += 1;
        if buffer.overflowed() || coverage == Coverage::Overflow {
            out.status = Status::Overflow;
            return out;
        }
        if coverage == Coverage::Unsupported {
            out.status = Status::Unsupported;
            return out;
        }
        if buffer.is_empty() {
            if backend.clear(candidate) {
                out.position = candidate;
                out.status = Status::Success;
            } else {
                out.status = Status::FinalRejected;
            }
            return out;
        }
        let correction = match minimum_correction(buffer.as_slice(), config.margin) {
            Ok(v) => v,
            Err(SolveError::InvalidInput) | Err(SolveError::Capacity) => {
                out.status = Status::InvalidInput;
                return out;
            }
            Err(SolveError::Infeasible) => {
                out.status = Status::Infeasible;
                return out;
            }
        };
        if iteration == config.iterations {
            out.status = Status::IterationLimit;
            return out;
        }
        let next = candidate + correction;
        if !next.finite() {
            out.status = Status::InvalidInput;
            return out;
        }
        if next == candidate || correction.length_squared() == 0.0 {
            out.status = Status::NoProgress;
            return out;
        }
        let displacement = (next - origin).length();
        if !displacement.is_finite() || displacement >= config.max_displacement {
            out.status = Status::BudgetExceeded;
            return out;
        }
        candidate = next;
        out.corrections += 1;
    }
    out.status = Status::IterationLimit;
    out
}
