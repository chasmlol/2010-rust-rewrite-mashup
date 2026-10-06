use sha1::{Digest, Sha1};

use crate::salsa20::apply_keystream;

pub const STREAM_COUNT: usize = 4;

const SLOTS_PER_STREAM: usize = 200;

const SLOT_LEN: usize = 20;

const TABLE_LEN: usize = STREAM_COUNT * SLOTS_PER_STREAM * SLOT_LEN;

const KEY: [u8; 32] = [
    0x64, 0x1D, 0x8A, 0x2F, 0xE3, 0x1D, 0x3A, 0xA6, 0x36, 0x22, 0xBB, 0xC9, 0xCE, 0x85, 0x87, 0x22,
    0x9D, 0x42, 0xB0, 0xF8, 0xED, 0x9B, 0x92, 0x41, 0x30, 0xBF, 0x88, 0xB6, 0x5E, 0xDC, 0x50, 0xBE,
];

pub struct ZoneCipher {
    table: [u8; TABLE_LEN],
    counters: [usize; STREAM_COUNT],
}

impl ZoneCipher {
    pub fn new(name: &[u8]) -> Self {
        let mut table = [0u8; TABLE_LEN];
        for (i, word) in table.as_chunks_mut::<4>().0.iter_mut().enumerate() {
            word.fill(name[i % name.len()]);
        }
        Self {
            table,
            counters: [0; STREAM_COUNT],
        }
    }

    fn slot(stream: usize, n: usize) -> usize {
        SLOT_LEN * (stream + STREAM_COUNT * (n % SLOTS_PER_STREAM))
    }

    /// Decrypts the chunk at `index` in place. Chunks must be fed in file
    /// order: every chunk advances its stream's IV.
    pub fn decrypt(&mut self, index: usize, data: &mut [u8]) {
        let stream = index % STREAM_COUNT;
        let n = self.counters[stream];
        let at = Self::slot(stream, n);
        let nonce: [u8; 8] = self.table[at..at + 8].try_into().unwrap();
        apply_keystream(&KEY, &nonce, data);

        let hash = Sha1::digest(&*data);
        let next = Self::slot(stream, n + 1);
        for (b, h) in self.table[next..next + SLOT_LEN].iter_mut().zip(hash) {
            *b ^= h;
        }
        self.counters[stream] = n + 1;
    }
}
