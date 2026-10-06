pub fn unpack_unit_vec(packed: u32) -> [f32; 3] {
    dpvs_iw4::skin_unpack_unit_vec(packed)
}

pub fn unpack_unit_vec_t6(packed: u32) -> [f32; 3] {
    let axis = |shift: u32| {
        let bits = (packed >> shift) & 0x3FF;
        let raw = bits
            .wrapping_sub(2 * (bits & 0x200))
            .wrapping_add(0x4040_0000);
        (f32::from_bits(raw) - 3.0) * 8208.031
    };
    [axis(0), axis(10), axis(20)]
}

pub fn pack_unit_vec(v: [f32; 3]) -> u32 {
    let n = normalize_or_up(v);
    let mut out = 0u32;
    let mut best_dir = f32::MAX;
    let mut best_len = f32::MAX;
    for scale_byte in 0..=255u8 {
        let encode = 32_385.0 / (f32::from(scale_byte) + 192.0);
        let byte = |c: f32| (c * encode + 127.5) as i32 as i8 as u8;
        let bytes = [byte(n[0]), byte(n[1]), byte(n[2]), scale_byte];
        let candidate = u32::from_le_bytes(bytes);
        let decoded = unpack_unit_vec(candidate);
        let len =
            (decoded[0] * decoded[0] + decoded[1] * decoded[1] + decoded[2] * decoded[2]).sqrt();
        let len_error = (len - 1.0).abs();
        if len_error >= 0.001 {
            continue;
        }
        let d = normalize_or_up(decoded);
        let dir_error = (d[0] * n[0] + d[1] * n[1] + d[2] * n[2] - 1.0).abs();
        if best_dir > dir_error || (best_dir <= dir_error && best_len > len_error) {
            best_dir = dir_error;
            best_len = len_error;
            out = candidate;
            if len_error + dir_error == 0.0 {
                break;
            }
        }
    }
    out
}

pub fn repack_vertex_t6(mut vertex: [u8; 32]) -> [u8; 32] {
    let word = |v: &[u8; 32], o: usize| u32::from_le_bytes(v[o..o + 4].try_into().unwrap());
    let uv = word(&vertex, 20);
    vertex[20..24].copy_from_slice(&uv.rotate_left(16).to_le_bytes());
    for o in [24, 28] {
        let packed = pack_unit_vec(unpack_unit_vec_t6(word(&vertex, o)));
        vertex[o..o + 4].copy_from_slice(&packed.to_le_bytes());
    }
    vertex
}

pub fn normalize_or_up(v: [f32; 3]) -> [f32; 3] {
    let length = (v[0] * v[0] + v[1] * v[1] + v[2] * v[2]).sqrt();
    if length == 0.0 {
        [0.0, 0.0, 1.0]
    } else {
        [v[0] / length, v[1] / length, v[2] / length]
    }
}

pub fn half_to_f32(bits: u16) -> f32 {
    let sign = u32::from(bits & 0x8000) << 16;
    let exponent = (bits >> 10) & 0x1f;
    let mantissa = u32::from(bits & 0x03ff);
    let value = match exponent {
        0 if mantissa == 0 => sign,
        0 => {
            let mut mantissa = mantissa;
            let mut exponent = -14i32;
            while mantissa & 0x0400 == 0 {
                mantissa <<= 1;
                exponent -= 1;
            }
            sign | (((exponent + 127) as u32) << 23) | ((mantissa & 0x03ff) << 13)
        }
        0x1f => sign | 0x7f80_0000 | (mantissa << 13),
        _ => sign | ((u32::from(exponent) + 112) << 23) | (mantissa << 13),
    };
    f32::from_bits(value)
}

pub fn unpack_packed_tex_coords(packed: u32) -> [f32; 2] {
    [
        half_to_f32((packed >> 16) as u16),
        half_to_f32(packed as u16),
    ]
}

pub fn unpack_color_u8(packed: u32) -> [u8; 4] {
    let [b, g, r, a] = packed.to_le_bytes();
    [r, g, b, a]
}

pub fn unpack_color(packed: u32) -> [f32; 4] {
    let [r, g, b, a] = unpack_color_u8(packed);
    [
        f32::from(r) / 255.0,
        f32::from(g) / 255.0,
        f32::from(b) / 255.0,
        f32::from(a) / 255.0,
    ]
}
