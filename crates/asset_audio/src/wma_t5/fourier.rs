use std::f64::consts::PI;
use std::sync::OnceLock;

#[derive(Clone, Copy, Default)]
struct Complex {
    real: f64,
    imaginary: f64,
}

impl Complex {
    fn phase(angle: f64) -> Self {
        let (imaginary, real) = angle.sin_cos();
        Self { real, imaginary }
    }
    fn times(self, rhs: Self) -> Self {
        Self {
            real: self.real * rhs.real - self.imaginary * rhs.imaginary,
            imaginary: self.real * rhs.imaginary + self.imaginary * rhs.real,
        }
    }
}

pub(super) struct Plan {
    twists: Vec<Complex>,
    rotations: Vec<Complex>,
    permutation: Vec<usize>,
    stages: Vec<Vec<Complex>>,
}

impl Plan {
    fn new(size: usize) -> Self {
        let count = 2 * size;
        let twists = (0..size)
            .map(|k| Complex::phase(PI * k as f64 / count as f64))
            .collect();
        let rotations = (0..count)
            .map(|m| Complex::phase(PI * (m as f64 + size as f64 / 2.0 + 0.5) / count as f64))
            .collect();
        let shift = usize::BITS - count.trailing_zeros();
        let permutation = (0..count).map(|i| i.reverse_bits() >> shift).collect();
        let stages = (1..=count.trailing_zeros())
            .map(|level| {
                let width = 1usize << level;
                (0..width / 2)
                    .map(|k| Complex::phase(2.0 * PI * k as f64 / width as f64))
                    .collect()
            })
            .collect();
        Self {
            twists,
            rotations,
            permutation,
            stages,
        }
    }

    pub fn inverse(&self, coefficients: &[f32], output: &mut [f32]) {
        let size = coefficients.len();
        let mut work = vec![Complex::default(); 2 * size];
        for (k, &sample) in coefficients.iter().enumerate() {
            let twist = self.twists[k];
            work[self.permutation[k]] = Complex {
                real: f64::from(sample) * twist.real,
                imaginary: f64::from(sample) * twist.imaginary,
            };
        }
        for stage in &self.stages {
            let half = stage.len();
            for group in work.chunks_exact_mut(2 * half) {
                for i in 0..half {
                    let left = group[i];
                    let right = group[i + half].times(stage[i]);
                    group[i] = Complex {
                        real: left.real + right.real,
                        imaginary: left.imaginary + right.imaginary,
                    };
                    group[i + half] = Complex {
                        real: left.real - right.real,
                        imaginary: left.imaginary - right.imaginary,
                    };
                }
            }
        }
        for (m, value) in output.iter_mut().enumerate() {
            *value = (-work[(m + size / 2) % (2 * size)]
                .times(self.rotations[m])
                .real
                / 32768.0) as f32;
        }
    }
}

pub(super) fn plan(size: usize) -> &'static Plan {
    static PLANS: OnceLock<Vec<Plan>> = OnceLock::new();
    &PLANS.get_or_init(|| (7..=11).map(|power| Plan::new(1 << power)).collect())
        [(size.trailing_zeros() - 7) as usize]
}

pub(super) fn window(sample: usize, size: usize, before: usize, after: usize) -> f32 {
    let (width, position) = if sample < size {
        let width = size.min(before);
        (width, sample as isize - ((size - width) / 2) as isize)
    } else {
        let width = size.min(after);
        (
            width,
            (2 * size - 1 - sample) as isize - ((size - width) / 2) as isize,
        )
    };
    if position < 0 {
        0.0
    } else if position >= width as isize {
        1.0
    } else {
        (PI * (position as f64 + 0.5) / (2 * width) as f64).sin() as f32
    }
}
