use std::sync::OnceLock;

use super::{Cursor, DecodeError, Result};

#[derive(Clone, Copy)]
pub(super) struct Word {
    boundary: u32,
    width: u8,
    pub operation: u8,
    pub zeros: u16,
    pub value: i16,
}

pub(super) struct Book(Vec<Word>);

impl Book {
    pub fn read(&self, input: &mut Cursor<'_>) -> Result<Word> {
        let key = input.preview(24);
        let index = self.0.partition_point(|word| word.boundary <= key);
        let word = *self
            .0
            .get(index.wrapping_sub(1))
            .ok_or_else(|| DecodeError::new("entropy prefix"))?;
        input.take(word.width as usize)?;
        Ok(word)
    }
}

pub(super) struct Format {
    pub books: Vec<Book>,
    bands: Vec<(u32, usize, Vec<usize>)>,
}

impl Format {
    pub fn bands(&self, rate: u32, size: usize) -> Result<&[usize]> {
        self.bands
            .iter()
            .find(|(r, n, _)| *r == rate && *n == size)
            .map(|(_, _, bands)| bands.as_slice())
            .ok_or_else(|| DecodeError::new("unsupported exponent grid"))
    }
}

pub(super) fn format() -> &'static Format {
    static FORMAT: OnceLock<Format> = OnceLock::new();
    FORMAT.get_or_init(|| {
        let mut data = &include_bytes!("format.bin")[8..];
        let word = |data: &mut &[u8], width: usize| -> u32 {
            let mut value = 0;
            for (i, byte) in data[..width].iter().enumerate() {
                value |= u32::from(*byte) << (8 * i);
            }
            *data = &data[width..];
            value
        };
        let count = word(&mut data, 2);
        let mut books = Vec::new();
        for _ in 0..count {
            let count = word(&mut data, 2);
            let mut entries = Vec::new();
            for _ in 0..count {
                entries.push(Word {
                    boundary: word(&mut data, 4),
                    width: word(&mut data, 1) as u8,
                    operation: word(&mut data, 1) as u8,
                    zeros: word(&mut data, 2) as u16,
                    value: word(&mut data, 2) as i16,
                });
            }
            books.push(Book(entries));
        }
        let count = word(&mut data, 2);
        let mut bands = Vec::new();
        for _ in 0..count {
            let rate = word(&mut data, 4);
            let size = word(&mut data, 2) as usize;
            let count = word(&mut data, 2);
            let widths = (0..count).map(|_| word(&mut data, 2) as usize).collect();
            bands.push((rate, size, widths));
        }
        assert!(data.is_empty());
        Format { books, bands }
    })
}
