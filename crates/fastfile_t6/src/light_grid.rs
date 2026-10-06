pub const COEFFICIENT_ROW_BYTES: usize = 54;

pub fn decode_coefficients(bytes: &[u8]) -> Option<[[f32; 3]; 9]> {
    let bytes = bytes.get(..COEFFICIENT_ROW_BYTES)?;
    let mut coefficients = [[0.0; 3]; 9];
    for (row, packed) in coefficients.iter_mut().zip(bytes.as_chunks::<6>().0) {
        for (out, channel) in row.iter_mut().zip(packed.as_chunks::<2>().0) {
            let value = u16::from_le_bytes(*channel) as f32;
            *out = libm::fmaf(value * f32::from_bits(0x37800080), 32.0, -16.0);
        }
    }
    Some(coefficients)
}

pub fn directional_colors(coefficients: &[[f32; 3]; 9]) -> [[f32; 3]; 56] {
    let mut colors = [[0.0; 3]; 56];
    let mut shell = 0;
    for z in 0..4 {
        for y in 0..4 {
            for x in 0..4 {
                if (1..3).contains(&x) && (1..3).contains(&y) && (1..3).contains(&z) {
                    continue;
                }
                let component = |i| libm::fmaf(i as f32, f32::from_bits(0x3f2aaaab), -1.0);
                let [x, y, z] = [component(x), component(y), component(z)];
                let length = libm::sqrtf(libm::fmaf(z, z, libm::fmaf(x, x, y * y)));
                let inverse = 1.0 / length;
                let [x, y, z] = [x * inverse, y * inverse, z * inverse];
                for c in 0..3 {
                    let mut value = libm::fmaf(coefficients[1][c], x, coefficients[0][c]);
                    value = libm::fmaf(coefficients[2][c], y, value);
                    value = libm::fmaf(coefficients[3][c], z, value);
                    value = libm::fmaf(coefficients[4][c], x * z, value);
                    value = libm::fmaf(coefficients[5][c], y * z, value);
                    value = libm::fmaf(coefficients[6][c], x * y, value);
                    value = libm::fmaf(coefficients[7][c], 3.0 * z * z - 1.0, value);
                    value = libm::fmaf(coefficients[8][c], x * x - y * y, value);
                    colors[shell][c] = value.max(0.0);
                }
                shell += 1;
            }
        }
    }
    colors
}

pub fn lighting_sh(coefficients: &[[f32; 3]; 9]) -> [[f32; 4]; 3] {
    let luminance = |[r, g, b]: [f32; 3]| (r + b) * 0.25 + g * 0.5;
    let ambient = luminance(coefficients[0]) + f32::from_bits(0x38d1b717);
    let zonal = luminance(coefficients[7]);
    let [r, g, b] = coefficients[0].map(|c| c / ambient);
    let l = |i: usize| luminance(coefficients[i]);
    [
        [r, g, b, zonal * 3.0],
        [l(1), l(2), l(3), ambient - zonal],
        [l(4), l(5), l(6), l(8)],
    ]
}
