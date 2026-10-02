//! A minimal reader for the binary glTF files the player's own tools write.
use serde::Serialize;
use serde_json::{Map, Value};

#[derive(Serialize)]
pub(crate) struct Texture {
    pub(crate) width: u32,
    pub(crate) height: u32,
    pub(crate) rgba: Vec<u8>,
}

pub(crate) fn index(value: &Value) -> Result<usize, String> {
    value
        .as_u64()
        .map(|v| v as usize)
        .ok_or_else(|| format!("expected an index, found {value}"))
}

pub(crate) struct Glb<'a> {
    pub(crate) json: Value,
    name: &'a str,
    blob: &'a [u8],
}

impl<'a> Glb<'a> {
    pub(crate) fn parse(bytes: &'a [u8], name: &'a str) -> Result<Self, String> {
        let word = |at: usize| -> Result<usize, String> {
            bytes
                .get(at..at + 4)
                .map(|b| u32::from_le_bytes([b[0], b[1], b[2], b[3]]) as usize)
                .ok_or_else(|| "truncated GLB".to_owned())
        };
        if bytes.get(..4) != Some(b"glTF") {
            return Err(format!("{name} is not a GLB file"));
        }
        let json_len = word(12)?;
        let json_bytes = bytes.get(20..20 + json_len).ok_or("truncated GLB JSON")?;
        let json = serde_json::from_slice(json_bytes).map_err(|error| error.to_string())?;
        let blob_len = word(20 + json_len)?;
        let blob_start = 28 + json_len;
        let blob = bytes
            .get(blob_start..blob_start + blob_len)
            .ok_or("truncated GLB binary chunk")?;
        Ok(Self { json, name, blob })
    }

    pub(crate) fn at(&self, path: &[&str]) -> Result<&Value, String> {
        let mut value = &self.json;
        for key in path {
            value = match key.parse::<usize>() {
                Ok(i) => &value[i],
                Err(_) => &value[*key],
            };
        }
        if value.is_null() {
            return Err(format!("{} has no {}", self.name, path.join("/")));
        }
        Ok(value)
    }

    fn view(&self, view: usize) -> Result<(&'a [u8], Option<usize>), String> {
        let view = &self.json["bufferViews"][view];
        let offset = view["byteOffset"].as_u64().unwrap_or(0) as usize;
        let length = index(&view["byteLength"])?;
        let bytes = self
            .blob
            .get(offset..offset + length)
            .ok_or("buffer view outside the GLB")?;
        Ok((bytes, view["byteStride"].as_u64().map(|s| s as usize)))
    }

    /// Every component of an accessor, widened to `f64`.
    fn components(&self, accessor: usize) -> Result<Vec<f64>, String> {
        let accessor: &Map<String, Value> = self.json["accessors"][accessor]
            .as_object()
            .ok_or("missing accessor")?;
        let count = index(&accessor["count"])?;
        let width = match accessor["type"].as_str() {
            Some("SCALAR") => 1,
            Some("VEC2") => 2,
            Some("VEC3") => 3,
            Some("VEC4") => 4,
            Some("MAT4") => 16,
            other => return Err(format!("unsupported accessor type {other:?}")),
        };
        let (size, read): (usize, fn(&[u8]) -> f64) = match accessor["componentType"].as_u64() {
            Some(5121) => (1, |b| f64::from(b[0])),
            Some(5123) => (2, |b| f64::from(u16::from_le_bytes([b[0], b[1]]))),
            Some(5125) => (4, |b| {
                f64::from(u32::from_le_bytes([b[0], b[1], b[2], b[3]]))
            }),
            Some(5126) => (4, |b| {
                f64::from(f32::from_le_bytes([b[0], b[1], b[2], b[3]]))
            }),
            other => return Err(format!("unsupported component type {other:?}")),
        };
        let (bytes, stride) = self.view(index(&accessor["bufferView"])?)?;
        let start = accessor
            .get("byteOffset")
            .and_then(Value::as_u64)
            .unwrap_or(0) as usize;
        let stride = stride.unwrap_or(size * width);
        let mut out = Vec::with_capacity(count * width);
        for element in 0..count {
            for component in 0..width {
                let at = start + element * stride + component * size;
                let field = bytes
                    .get(at..at + size)
                    .ok_or("accessor outside its view")?;
                out.push(read(field));
            }
        }
        Ok(out)
    }

    pub(crate) fn floats(&self, accessor: usize) -> Result<Vec<f32>, String> {
        Ok(self
            .components(accessor)?
            .into_iter()
            .map(|v| v as f32)
            .collect())
    }

    pub(crate) fn integers(&self, accessor: usize) -> Result<Vec<u32>, String> {
        Ok(self
            .components(accessor)?
            .into_iter()
            .map(|v| v as u32)
            .collect())
    }

    /// Image `image` as RGBA, box-filtered down to fit `limit` on both sides.
    pub(crate) fn texture(&self, image: usize, limit: u32) -> Result<Texture, String> {
        let source = &self.json["images"][image];
        let inline;
        let bytes = match source["uri"].as_str() {
            Some(uri) => {
                let data = uri
                    .split_once(";base64,")
                    .map(|(_, data)| data)
                    .ok_or_else(|| format!("{} image {image} is not embedded", self.name))?;
                inline = base64(data)
                    .ok_or_else(|| format!("{} image {image}: bad base64", self.name))?;
                &inline[..]
            }
            None => self.view(index(&source["bufferView"])?)?.0,
        };
        let mut decoder = png::Decoder::new(std::io::Cursor::new(bytes));
        decoder.set_transformations(png::Transformations::normalize_to_color8());
        let mut reader = decoder
            .read_info()
            .map_err(|error| format!("{} image {image}: {error}", self.name))?;
        let mut pixels = vec![0; reader.output_buffer_size().ok_or("image too large")?];
        let frame = reader
            .next_frame(&mut pixels)
            .map_err(|error| format!("{} image {image}: {error}", self.name))?;
        let channels = frame.color_type.samples();
        let rgba: Vec<u8> = pixels[..frame.buffer_size()]
            .chunks_exact(channels)
            .flat_map(|p| match channels {
                1 => [p[0], p[0], p[0], 255],
                2 => [p[0], p[0], p[0], p[1]],
                3 => [p[0], p[1], p[2], 255],
                _ => [p[0], p[1], p[2], p[3]],
            })
            .collect();
        Ok(shrink(frame.width, frame.height, rgba, limit))
    }
}

/// Standard base64, padding optional.
fn base64(text: &str) -> Option<Vec<u8>> {
    let value = |c: u8| -> Option<u32> {
        Some(match c {
            b'A'..=b'Z' => c - b'A',
            b'a'..=b'z' => c - b'a' + 26,
            b'0'..=b'9' => c - b'0' + 52,
            b'+' => 62,
            b'/' => 63,
            _ => return None,
        } as u32)
    };
    let text = text.trim_end_matches('=').as_bytes();
    let mut out = Vec::with_capacity(text.len() * 3 / 4);
    for chunk in text.chunks(4) {
        let mut word = 0u32;
        for (i, &c) in chunk.iter().enumerate() {
            word |= value(c)? << (18 - 6 * i);
        }
        let bytes = word.to_be_bytes();
        out.extend_from_slice(&bytes[1..chunk.len()]);
    }
    Some(out)
}

/// Box-filters an image to fit within `limit` on both sides, keeping its
/// aspect ratio. Smaller images pass through.
fn shrink(width: u32, height: u32, rgba: Vec<u8>, limit: u32) -> Texture {
    if width <= limit && height <= limit {
        return Texture {
            width,
            height,
            rgba,
        };
    }
    let scale = (f64::from(limit) / f64::from(width)).min(f64::from(limit) / f64::from(height));
    let out_w = ((f64::from(width) * scale).round() as u32).max(1);
    let out_h = ((f64::from(height) * scale).round() as u32).max(1);
    let mut out = Vec::with_capacity((out_w * out_h * 4) as usize);
    for y in 0..out_h {
        let y0 = y * height / out_h;
        let y1 = ((y + 1) * height / out_h).max(y0 + 1);
        for x in 0..out_w {
            let x0 = x * width / out_w;
            let x1 = ((x + 1) * width / out_w).max(x0 + 1);
            let mut sum = [0u32; 4];
            for sy in y0..y1 {
                for sx in x0..x1 {
                    let at = ((sy * width + sx) * 4) as usize;
                    for c in 0..4 {
                        sum[c] += u32::from(rgba[at + c]);
                    }
                }
            }
            let n = (y1 - y0) * (x1 - x0);
            out.extend(sum.map(|s| ((s + n / 2) / n) as u8));
        }
    }
    Texture {
        width: out_w,
        height: out_h,
        rgba: out,
    }
}
