#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Hit {
    pub fraction: f64,
    pub normal: [f64; 3],
    pub startsolid: bool,
    pub allsolid: bool,
}

fn valid(v: [f64; 3]) -> bool {
    v.iter().all(|x| x.is_finite())
}
fn dot(a: [f64; 3], b: [f64; 3]) -> f64 {
    libm::fma(a[0], b[0], libm::fma(a[1], b[1], a[2] * b[2]))
}
fn offset(p: [f64; 3], h: f64) -> [f64; 3] {
    [
        p[0],
        p[1],
        if p[2] > h {
            p[2] - h
        } else if p[2] < -h {
            p[2] + h
        } else {
            0.0
        },
    ]
}
fn length(p: [f64; 3]) -> f64 {
    libm::hypot(libm::hypot(p[0], p[1]), p[2])
}
fn normal(p: [f64; 3], h: f64, d: [f64; 3]) -> [f64; 3] {
    let w = offset(p, h);
    let n = length(w);
    if n > 0.0 {
        return [w[0] / n, w[1] / n, w[2] / n];
    }
    let n = length(d);
    if n > 0.0 {
        [-d[0] / n, -d[1] / n, -d[2] / n]
    } else {
        [0.0, 0.0, 1.0]
    }
}
fn point(s: [f64; 3], d: [f64; 3], t: f64) -> [f64; 3] {
    [
        libm::fma(d[0], t, s[0]),
        libm::fma(d[1], t, s[1]),
        libm::fma(d[2], t, s[2]),
    ]
}

fn roots(a: f64, b: f64, c: f64, geometric_disc: f64, disc_error: f64) -> Option<[f64; 2]> {
    if a == 0.0 {
        if b == 0.0 {
            return if c == 0.0 { Some([0.0, 0.0]) } else { None };
        }
        let t = -c / b;
        return Some([t, t]);
    }
    let mut disc = geometric_disc;
    let error = disc_error;
    if disc < 0.0 {
        if disc < -error {
            return None;
        }
        disc = 0.0;
    }
    let root = libm::sqrt(disc);
    let q = -0.5 * (b + libm::copysign(root, b));
    if q == 0.0 {
        let t = -b / (2.0 * a);
        return Some([t, t]);
    }
    let x = q / a;
    let y = c / q;
    Some(if x <= y { [x, y] } else { [y, x] })
}

pub fn sweep_point(start: [f64; 3], end: [f64; 3], radius: f64, half_segment: f64) -> Option<Hit> {
    if !valid(start)
        || !valid(end)
        || !radius.is_finite()
        || !half_segment.is_finite()
        || radius < 0.0
        || half_segment < 0.0
    {
        return None;
    }
    let effective_h = half_segment.min(start[2].abs().max(end[2].abs()));
    let mut scale = radius.max(effective_h);
    for i in 0..3 {
        scale = scale.max(start[i].abs()).max(end[i].abs());
    }
    if scale == 0.0 {
        return None;
    }
    let s = start.map(|x| x / scale);
    let e = end.map(|x| x / scale);
    let r = radius / scale;
    let h = effective_h / scale;
    let d = [e[0] - s[0], e[1] - s[1], e[2] - s[2]];
    let w = offset(s, h);
    let initial_distance = length(w);
    let finish = length(offset(e, h));
    if initial_distance < r {
        return Some(Hit {
            fraction: 0.0,
            normal: normal(s, h, d),
            startsolid: true,
            allsolid: finish < r,
        });
    }
    if initial_distance == r {
        return if dot(w, d) < 0.0 && r > 0.0 {
            Some(Hit {
                fraction: 0.0,
                normal: normal(s, h, d),
                startsolid: false,
                allsolid: false,
            })
        } else {
            None
        };
    }

    let mut cuts = [0.0, 1.0, 1.0, 1.0];
    let mut count = 2;
    if d[2] != 0.0 {
        for z in [-h, h] {
            let t = (z - s[2]) / d[2];
            if t > 0.0 && t < 1.0 {
                cuts[count] = t;
                count += 1;
            }
        }
    }
    for i in 1..count {
        let mut j = i;
        while j > 0 && cuts[j] < cuts[j - 1] {
            cuts.swap(j, j - 1);
            j -= 1;
        }
    }
    for i in 0..count - 1 {
        let lo = cuts[i];
        let hi = cuts[i + 1];
        if hi <= lo {
            continue;
        }
        let middle_z = libm::fma(d[2], lo + (hi - lo) * 0.5, s[2]);
        let mut p = point(s, d, lo);
        let mut v = d;
        if middle_z > h {
            p[2] -= h;
        } else if middle_z < -h {
            p[2] += h;
        } else {
            p[2] = 0.0;
            v[2] = 0.0;
        }
        let a = dot(v, v);
        let b = 2.0 * dot(p, v);
        let c = libm::fma(
            p[0],
            p[0],
            libm::fma(p[1], p[1], libm::fma(p[2], p[2], -r * r)),
        );
        fn product_difference(x: f64, y: f64, z: f64, w: f64) -> f64 {
            let zw = z * w;
            libm::fma(x, y, -zw) - libm::fma(z, w, -zw)
        }
        let cross = [
            product_difference(p[1], v[2], p[2], v[1]),
            product_difference(p[2], v[0], p[0], v[2]),
            product_difference(p[0], v[1], p[1], v[0]),
        ];
        let cross_sq = dot(cross, cross);
        let rr = r * r;
        let quarter_disc = libm::fma(rr, a, -cross_sq);
        let disc_error = 64.0 * f64::EPSILON * (rr * a + cross_sq);
        let Some(candidates) = roots(a, b, c, 4.0 * quarter_disc, disc_error) else {
            continue;
        };
        for u in candidates {
            let t = lo + u;
            let join_error = 32.0 * f64::EPSILON * (1.0 + lo.abs() + hi.abs() + u.abs());
            if !t.is_finite() || t < lo - join_error || t > hi + join_error {
                continue;
            }
            let t = t.clamp(lo, hi);
            let contact = point(s, d, t);
            let distance = length(offset(contact, h));
            let residual_error = 64.0 * f64::EPSILON * (r + length(s) + length(d));
            if (distance - r).abs() > residual_error {
                continue;
            }
            return Some(Hit {
                fraction: t,
                normal: if r == 0.0 {
                    normal([0.0; 3], h, d)
                } else {
                    normal(contact, h, d)
                },
                startsolid: false,
                allsolid: false,
            });
        }
    }
    None
}
