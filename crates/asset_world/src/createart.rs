use std::collections::HashMap;
use std::f32::consts::LN_2;

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ExpFog {
    pub start_dist: f32,

    pub halfway_dist: f32,
    pub color_rgb: [f32; 3],

    pub max_opacity: f32,

    pub transition_time: f32,

    pub sun: Option<SunFog>,

    pub volumetric: Option<VolFog>,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct VolFog {
    pub halfway_height: f32,
    pub base_height: f32,
    pub color_scale: f32,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SunFog {
    pub color_rgb: [f32; 3],

    pub sun_dir: [f32; 3],
    pub begin_angle_deg: f32,
    pub end_angle_deg: f32,

    pub scale: f32,
}

impl ExpFog {
    pub fn density(&self) -> f32 {
        if self.halfway_dist <= 0.0 {
            return 0.0;
        }
        LN_2 / self.halfway_dist
    }
}

pub fn parse_set_exp_fog(source: &str) -> Option<ExpFog> {
    let lower = source.to_ascii_lowercase();
    let key = "setexpfog";
    let start = lower.find(key)?;
    let after = &source[start + key.len()..];
    let open = after.find('(')?;
    let mut depth = 0i32;
    let mut close_rel = None;
    for (i, ch) in after[open..].char_indices() {
        match ch {
            '(' => depth += 1,
            ')' => {
                depth -= 1;
                if depth == 0 {
                    close_rel = Some(open + i);
                    break;
                }
            }
            _ => {}
        }
    }
    let inside = after[open + 1..close_rel?].trim();

    let mut args: Vec<&str> = Vec::new();
    let mut depth = 0i32;
    let mut field_start = 0usize;
    for (i, ch) in inside.char_indices() {
        match ch {
            '(' | '[' => depth += 1,
            ')' | ']' => depth -= 1,
            ',' if depth == 0 => {
                args.push(inside[field_start..i].trim());
                field_start = i + 1;
            }
            _ => {}
        }
    }
    args.push(inside[field_start..].trim());

    let scalar = |i: usize| {
        args.get(i)
            .and_then(|s| s.parse::<f32>().ok())
            .filter(|v| v.is_finite())
    };
    let vec3 = |i: usize| -> Option<[f32; 3]> {
        let s = args
            .get(i)?
            .trim_matches(|c| c == '(' || c == ')' || c == '[' || c == ']');
        let n: Vec<f32> = s
            .split(',')
            .map(|p| p.trim().parse::<f32>().ok().filter(|v| v.is_finite()))
            .collect::<Option<Vec<_>>>()?;
        match n.as_slice() {
            &[x, y, z] => Some([x, y, z]),
            _ => None,
        }
    };

    let start_dist = scalar(0)?;
    let halfway_dist = scalar(1)?;
    let r = scalar(2)?;
    let g = scalar(3)?;
    let b = scalar(4)?;

    if start_dist < 0.0 || halfway_dist <= 0.0 || ![r, g, b].iter().all(|v| (0.0..=1.0).contains(v))
    {
        return None;
    }
    let (max_opacity, transition_time) = match args.len() {
        6 => (1.0, scalar(5)?),
        7 | 14 => (scalar(5)?, scalar(6)?),
        _ => return None,
    };
    if !(0.0..=1.0).contains(&max_opacity) || transition_time < 0.0 {
        return None;
    }

    if args.len() == 14
        && let (Some(sr), Some(sg), Some(sb), Some(dir), Some(begin), Some(end), Some(sun_scale)) = (
            scalar(7),
            scalar(8),
            scalar(9),
            vec3(10),
            scalar(11),
            scalar(12),
            scalar(13),
        )
    {
        if ![sr, sg, sb].iter().all(|v| (0.0..=1.0).contains(v))
            || !(0.0..=180.0).contains(&begin)
            || !(begin..=180.0).contains(&end)
            || sun_scale < 0.0
        {
            return None;
        }
        return Some(ExpFog {
            start_dist,
            halfway_dist,
            color_rgb: [r, g, b],
            max_opacity,
            transition_time,
            volumetric: None,
            sun: Some(SunFog {
                color_rgb: [sr, sg, sb],
                sun_dir: dir,
                begin_angle_deg: begin,
                end_angle_deg: end,
                scale: sun_scale,
            }),
        });
    }

    match args.len() {
        7 => Some(ExpFog {
            start_dist,
            halfway_dist,
            color_rgb: [r, g, b],
            max_opacity,
            transition_time,
            sun: None,
            volumetric: None,
        }),
        6 => Some(ExpFog {
            start_dist,
            halfway_dist,
            color_rgb: [r, g, b],
            max_opacity: 1.0,
            transition_time,
            sun: None,
            volumetric: None,
        }),
        _ => None,
    }
}

pub fn is_createart_source(name: &str) -> bool {
    let n = name.replace('\\', "/").to_ascii_lowercase();
    n.contains("createart/")
}

pub fn parse_createart_rawfile(name: &str, data: &[u8], zlib_compressed: bool) -> Option<ExpFog> {
    if !is_createart_source(name) {
        return None;
    }
    let bytes = decode_createart_bytes(data, zlib_compressed)?;
    let text = std::str::from_utf8(&bytes).ok()?;
    let stripped = strip_gsc_comments(text);
    parse_set_exp_fog(&stripped)
        .or_else(|| parse_vision_set_fog(&stripped))
        .or_else(|| parse_set_vol_fog(&stripped))
}

fn decode_createart_bytes(data: &[u8], zlib_compressed: bool) -> Option<Vec<u8>> {
    if zlib_compressed {
        return asset_transport::inflate_zlib(data).ok();
    }
    let trimmed = data.strip_suffix(&[0]).unwrap_or(data);
    if std::str::from_utf8(trimmed).is_ok() {
        return Some(trimmed.to_vec());
    }
    decode_packed_rawfile(data)
}

pub fn decode_rawfile_text(data: &[u8], zlib_compressed: bool) -> Option<String> {
    let bytes = decode_createart_bytes(data, zlib_compressed)?;
    std::str::from_utf8(&bytes)
        .ok()
        .map(|s| s.trim_end_matches('\0').to_owned())
}

pub fn decode_packed_rawfile(data: &[u8]) -> Option<Vec<u8>> {
    let data = data.strip_suffix(&[0]).unwrap_or(data);
    if data.len() < 8 {
        return None;
    }
    let uncompressed = u32::from_le_bytes(data[0..4].try_into().ok()?) as usize;
    let compressed = u32::from_le_bytes(data[4..8].try_into().ok()?) as usize;
    if compressed == 0 || data.len() < 8 + compressed {
        return None;
    }
    let zlib = &data[8..8 + compressed];
    if zlib.first() != Some(&0x78) {
        return None;
    }
    let out = asset_transport::inflate_zlib(zlib).ok()?;
    if uncompressed != 0 && out.len() != uncompressed {
        return None;
    }
    Some(out)
}

fn strip_gsc_comments(source: &str) -> String {
    let b = source.as_bytes();
    let mut out = Vec::with_capacity(b.len());
    let mut i = 0;
    let mut in_str = false;
    while i < b.len() {
        if in_str {
            out.push(b[i]);
            if b[i] == b'\\' && i + 1 < b.len() {
                out.push(b[i + 1]);
                i += 2;
                continue;
            }
            if b[i] == b'"' {
                in_str = false;
            }
            i += 1;
            continue;
        }
        if b[i] == b'"' {
            in_str = true;
            out.push(b[i]);
            i += 1;
            continue;
        }
        if b[i] == b'/' && i + 1 < b.len() && b[i + 1] == b'/' {
            while i < b.len() && b[i] != b'\n' {
                i += 1;
            }
            continue;
        }
        if b[i] == b'/' && i + 1 < b.len() && b[i + 1] == b'*' {
            i += 2;
            while i + 1 < b.len() && !(b[i] == b'*' && b[i + 1] == b'/') {
                i += 1;
            }
            i = i.saturating_add(2).min(b.len());
            out.push(b' ');
            continue;
        }
        out.push(b[i]);
        i += 1;
    }
    String::from_utf8(out).unwrap_or_default()
}

fn parse_gsc_scalar_assigns(source: &str) -> HashMap<String, f32> {
    let mut map = HashMap::new();
    for line in source.lines() {
        let line = line.trim();
        let Some((lhs, rhs)) = line.split_once('=') else {
            continue;
        };
        let name = lhs.trim();
        if name.is_empty() || !name.chars().all(|c| c.is_ascii_alphanumeric() || c == '_') {
            continue;
        }
        let rhs = rhs.trim().trim_end_matches(';').trim();
        if let Ok(v) = rhs.parse::<f32>() {
            map.insert(name.to_ascii_lowercase(), v);
        }
    }
    map
}

fn first_call_args<'a>(source: &'a str, fn_name: &str) -> Option<Vec<&'a str>> {
    let lower = source.to_ascii_lowercase();
    let key = fn_name.to_ascii_lowercase();
    let start = lower.find(&key)?;
    let after = &source[start + key.len()..];
    let open = after.find('(')?;
    let mut depth = 0i32;
    let mut close_rel = None;
    for (i, ch) in after[open..].char_indices() {
        match ch {
            '(' => depth += 1,
            ')' => {
                depth -= 1;
                if depth == 0 {
                    close_rel = Some(open + i);
                    break;
                }
            }
            _ => {}
        }
    }
    let inside = after[open + 1..close_rel?].trim();
    let mut args: Vec<&str> = Vec::new();
    let mut depth = 0i32;
    let mut field_start = 0usize;
    for (i, ch) in inside.char_indices() {
        match ch {
            '(' | '[' => depth += 1,
            ')' | ']' => depth -= 1,
            ',' if depth == 0 => {
                args.push(inside[field_start..i].trim());
                field_start = i + 1;
            }
            _ => {}
        }
    }
    args.push(inside[field_start..].trim());
    Some(args)
}

fn resolve_scalar(token: &str, assigns: &HashMap<String, f32>) -> Option<f32> {
    let t = token.trim();
    t.parse()
        .ok()
        .or_else(|| assigns.get(&t.to_ascii_lowercase()).copied())
}

pub fn parse_set_vol_fog(source: &str) -> Option<ExpFog> {
    let stripped = strip_gsc_comments(source);
    let assigns = parse_gsc_scalar_assigns(&stripped);
    let args = first_call_args(&stripped, "setVolFog")?;
    let scalars: Vec<f32> = args
        .iter()
        .map(|s| resolve_scalar(s, &assigns))
        .collect::<Option<_>>()?;
    if let Ok(scalars) = <[f32; 18]>::try_from(scalars.as_slice()) {
        return Some(vol_fog(scalars));
    }
    let [
        start_dist,
        halfway_dist,
        halfway_height,
        base_height,
        r,
        g,
        b,
        transition_time,
    ] = <[f32; 8]>::try_from(scalars.as_slice()).ok()?;
    let color_scale = r.max(g).max(b);
    Some(ExpFog {
        start_dist,
        halfway_dist,
        color_rgb: [r, g, b].map(|c| c / color_scale),
        max_opacity: 1.0,
        transition_time,
        sun: Some(SunFog {
            color_rgb: [0.5; 3],
            sun_dir: [1.0, 0.0, 0.0],
            begin_angle_deg: 0.0,
            end_angle_deg: 0.0,
            scale: 1.0,
        }),
        volumetric: Some(VolFog {
            halfway_height,
            base_height,
            color_scale,
        }),
    })
}

pub fn vol_fog(v: [f32; 18]) -> ExpFog {
    ExpFog {
        start_dist: v[0],
        halfway_dist: v[1],
        color_rgb: [v[4], v[5], v[6]],
        max_opacity: v[17],
        transition_time: v[16],
        sun: Some(SunFog {
            color_rgb: [v[8], v[9], v[10]],
            sun_dir: [v[11], v[12], v[13]],
            begin_angle_deg: v[14],
            end_angle_deg: v[15],
            scale: 1.0,
        }),
        volumetric: Some(VolFog {
            halfway_height: v[2],
            base_height: v[3],
            color_scale: v[7],
        }),
    }
}

fn field_f32(source: &str, field: &str) -> Option<f32> {
    let lower = source.to_ascii_lowercase();
    let needle = format!(".{field}").to_ascii_lowercase();
    let at = lower.find(&needle)?;
    let after = &source[at + needle.len()..];
    let eq = after.find('=')?;
    let rest = after[eq + 1..].trim_start();
    let end = rest
        .find(|c: char| c == ';' || c == '\n' || c == '\r')
        .unwrap_or(rest.len());
    rest[..end].trim().parse().ok()
}

fn field_vec3(source: &str, field: &str) -> Option<[f32; 3]> {
    let lower = source.to_ascii_lowercase();
    let needle = format!(".{field}").to_ascii_lowercase();
    let at = lower.find(&needle)?;
    let after = &source[at + needle.len()..];
    let eq = after.find('=')?;
    let rest = after[eq + 1..].trim_start();
    let end = rest.find(';').unwrap_or(rest.len());
    let s = rest[..end].trim_matches(|c| c == '(' || c == ')' || c == ' ' || c == '\t');
    let n: Vec<f32> = s
        .split(',')
        .filter_map(|p| p.trim().parse::<f32>().ok())
        .collect();
    match n.as_slice() {
        &[x, y, z] => Some([x, y, z]),
        _ => None,
    }
}

pub fn parse_vision_set_fog(source: &str) -> Option<ExpFog> {
    if !source
        .to_ascii_lowercase()
        .contains("create_vision_set_fog")
    {
        return None;
    }
    let start_dist = field_f32(source, "startDist")?;
    let halfway_dist = field_f32(source, "halfwayDist")?;
    let r = field_f32(source, "red")?;
    let g = field_f32(source, "green")?;
    let b = field_f32(source, "blue")?;
    let sun = if field_f32(source, "sunFogEnabled").unwrap_or(0.0) != 0.0 {
        let color = [
            field_f32(source, "sunRed")?,
            field_f32(source, "sunGreen")?,
            field_f32(source, "sunBlue")?,
        ];
        let sun_dir = field_vec3(source, "sunDir")?;
        Some(SunFog {
            color_rgb: color,
            sun_dir,
            begin_angle_deg: field_f32(source, "sunBeginFadeAngle")?,
            end_angle_deg: field_f32(source, "sunEndFadeAngle")?,
            scale: field_f32(source, "normalFogScale")
                .or_else(|| field_f32(source, "sunFogScale"))?,
        })
    } else {
        None
    };
    Some(ExpFog {
        start_dist,
        halfway_dist,
        color_rgb: [r, g, b],
        max_opacity: field_f32(source, "maxOpacity").unwrap_or(1.0),
        transition_time: field_f32(source, "transitionTime").unwrap_or(0.0),
        sun,
        volumetric: None,
    })
}

pub fn is_createart_fog_file(name: &str) -> bool {
    let n = name.replace('\\', "/").to_ascii_lowercase();
    let base = n.rsplit('/').next().unwrap_or(&n);
    base.contains("_fog")
}

const VOL_FOG_LOCALS: [&str; 18] = [
    "start_dist",
    "half_dist",
    "half_height",
    "base_height",
    "fog_r",
    "fog_g",
    "fog_b",
    "fog_scale",
    "sun_col_r",
    "sun_col_g",
    "sun_col_b",
    "sun_dir_x",
    "sun_dir_y",
    "sun_dir_z",
    "sun_start_ang",
    "sun_stop_ang",
    "time",
    "max_fog_opacity",
];

pub fn t6_set_vol_fog(bytes: &[u8]) -> Option<ExpFog> {
    if bytes.get(..8)? != b"\x80GSC\r\n\0\x06" {
        return None;
    }
    let word = |at| Some(u32::from_le_bytes(bytes.get(at..at + 4)?.try_into().ok()?));
    let half = |at| Some(u16::from_le_bytes(bytes.get(at..at + 2)?.try_into().ok()?));
    let mut locals = HashMap::new();
    let mut at = word(24)? as usize;
    for _ in 0..half(50)? {
        let offset = half(at)? as usize;
        let count = usize::from(*bytes.get(at + 2)?);
        let kind = *bytes.get(at + 3)?;
        let tail = bytes.get(offset..)?;
        let name = std::str::from_utf8(tail.get(..tail.iter().position(|&b| b == 0)?)?).ok()?;
        if kind == 1 && count == 1 {
            locals.insert(name, word(at + 4)? as usize);
        }
        at += 4 + 4 * count;
    }
    let table = *locals.get(VOL_FOG_LOCALS[0])?;
    let local_count = usize::from(*bytes.get(table - 1)?);
    let code = table + 2 * local_count;
    let mut values = [0.0; 18];
    for (value, name) in values.iter_mut().zip(VOL_FOG_LOCALS) {
        let position = (*locals.get(name)?).checked_sub(table)? / 2;
        let index = u8::try_from(local_count.checked_sub(position + 1)?).ok()?;
        let set = code
            + bytes
                .get(code..)?
                .windows(3)
                .position(|w| w == [0x27, index, 0x28])?;
        let float = (set % 4 == 0)
            .then(|| {
                (set.checked_sub(7)?..set - 4)
                    .find(|&p| bytes[p] == 0x09 && (p + 4) & !3 == set - 4)
            })
            .flatten();
        *value = match (float, bytes.get(set.checked_sub(2)?..set)?) {
            (Some(_), _) => f32::from_le_bytes(bytes[set - 4..set].try_into().ok()?),
            (None, [_, 0x03]) => 0.0,
            (None, [0x04, byte]) => f32::from(*byte),
            _ => return None,
        };
    }
    Some(vol_fog(values))
}
