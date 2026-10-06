const SIGMA: [u32; 4] = [0x6170_7865, 0x3320_646e, 0x7962_2d32, 0x6b20_6574];

fn quarter(x: &mut [u32; 16], a: usize, b: usize, c: usize, d: usize) {
    x[b] ^= x[a].wrapping_add(x[d]).rotate_left(7);
    x[c] ^= x[b].wrapping_add(x[a]).rotate_left(9);
    x[d] ^= x[c].wrapping_add(x[b]).rotate_left(13);
    x[a] ^= x[d].wrapping_add(x[c]).rotate_left(18);
}

fn block(key: &[u32; 8], nonce: [u32; 2], counter: u64) -> [u8; 64] {
    let state = [
        SIGMA[0],
        key[0],
        key[1],
        key[2],
        key[3],
        SIGMA[1],
        nonce[0],
        nonce[1],
        counter as u32,
        (counter >> 32) as u32,
        SIGMA[2],
        key[4],
        key[5],
        key[6],
        key[7],
        SIGMA[3],
    ];
    let mut x = state;
    for _ in 0..10 {
        quarter(&mut x, 0, 4, 8, 12);
        quarter(&mut x, 5, 9, 13, 1);
        quarter(&mut x, 10, 14, 2, 6);
        quarter(&mut x, 15, 3, 7, 11);
        quarter(&mut x, 0, 1, 2, 3);
        quarter(&mut x, 5, 6, 7, 4);
        quarter(&mut x, 10, 11, 8, 9);
        quarter(&mut x, 15, 12, 13, 14);
    }
    let mut out = [0u8; 64];
    for (i, word) in x.iter().enumerate() {
        out[i * 4..i * 4 + 4].copy_from_slice(&word.wrapping_add(state[i]).to_le_bytes());
    }
    out
}

pub(crate) fn apply_keystream(key: &[u8; 32], nonce: &[u8; 8], data: &mut [u8]) {
    let key: [u32; 8] =
        core::array::from_fn(|i| u32::from_le_bytes(key[i * 4..i * 4 + 4].try_into().unwrap()));
    let nonce = [
        u32::from_le_bytes(nonce[0..4].try_into().unwrap()),
        u32::from_le_bytes(nonce[4..8].try_into().unwrap()),
    ];
    for (counter, chunk) in data.chunks_mut(64).enumerate() {
        let ks = block(&key, nonce, counter as u64);
        for (b, k) in chunk.iter_mut().zip(ks) {
            *b ^= k;
        }
    }
}
