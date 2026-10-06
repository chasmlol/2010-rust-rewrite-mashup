#[derive(Clone, Copy, Debug, PartialEq)]
pub struct FilmVision {
    pub enable: bool,
    pub contrast: f32,
    pub brightness: f32,
    pub desaturation: f32,
    pub desaturation_dark: f32,
    pub invert: bool,
    pub light_tint: [f32; 3],
    pub medium_tint: [f32; 3],
    pub dark_tint: [f32; 3],
    pub glow_enable: bool,
    pub glow_radius: f32,
    pub glow_bloom_cutoff: f32,
    pub glow_bloom_desaturation: f32,
    pub glow_bloom_intensity: f32,
}

impl Default for FilmVision {
    fn default() -> Self {
        Self {
            enable: false,
            contrast: 1.0,
            brightness: 0.0,
            desaturation: 0.0,
            desaturation_dark: 0.0,
            invert: false,
            light_tint: [1.0; 3],
            medium_tint: [1.0; 3],
            dark_tint: [1.0; 3],
            glow_enable: false,
            glow_radius: 0.0,
            glow_bloom_cutoff: 0.0,
            glow_bloom_desaturation: 0.0,
            glow_bloom_intensity: 0.0,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FilmVisionParseError {
    Decode,
    InvalidScalar(&'static str),
    InvalidVector(&'static str),
    MissingField(&'static str),
}

fn is_vision_source(name: &str) -> bool {
    let name = name.replace('\\', "/").to_ascii_lowercase();
    name.starts_with("vision/") && name.ends_with(".vision")
}

fn scalar(value: &str, key: &'static str) -> Result<f32, FilmVisionParseError> {
    value
        .trim()
        .trim_matches('"')
        .parse()
        .map_err(|_| FilmVisionParseError::InvalidScalar(key))
}

fn vector(value: &str, key: &'static str) -> Result<[f32; 3], FilmVisionParseError> {
    let value = value.trim().trim_matches('"');
    let mut values = value.split_whitespace();
    let x = values
        .next()
        .ok_or(FilmVisionParseError::InvalidVector(key))?
        .parse()
        .map_err(|_| FilmVisionParseError::InvalidVector(key))?;
    let y = values
        .next()
        .ok_or(FilmVisionParseError::InvalidVector(key))?
        .parse()
        .map_err(|_| FilmVisionParseError::InvalidVector(key))?;
    let z = values
        .next()
        .ok_or(FilmVisionParseError::InvalidVector(key))?
        .parse()
        .map_err(|_| FilmVisionParseError::InvalidVector(key))?;
    if values.next().is_some() {
        return Err(FilmVisionParseError::InvalidVector(key));
    }
    Ok([x, y, z])
}

pub fn parse_film_vision_rawfile(
    name: &str,
    data: &[u8],
    zlib_compressed: bool,
) -> Result<Option<FilmVision>, FilmVisionParseError> {
    if !is_vision_source(name) {
        return Ok(None);
    }
    let source =
        super::decode_rawfile_text(data, zlib_compressed).ok_or(FilmVisionParseError::Decode)?;
    let mut enable = None;
    let mut contrast = None;
    let mut brightness = None;
    let mut desaturation = None;
    let mut desaturation_dark = None;
    let mut invert = None;
    let mut light_tint = None;
    let mut medium_tint = None;
    let mut dark_tint = None;
    let mut glow_enable = None;
    let mut glow_radius = None;
    let mut glow_bloom_cutoff = None;
    let mut glow_bloom_desaturation = None;
    let mut glow_bloom_intensity = None;
    for line in source.lines().map(str::trim) {
        if line.is_empty() || line.starts_with("//") {
            continue;
        }
        let Some(split) = line.find(char::is_whitespace) else {
            continue;
        };
        let value = line[split..].trim();
        match line[..split].to_ascii_lowercase().as_str() {
            "r_filmenable" => enable = Some(scalar(value, "r_filmEnable")? != 0.0),
            "r_filmcontrast" => contrast = Some(scalar(value, "r_filmContrast")?),
            "r_filmbrightness" => brightness = Some(scalar(value, "r_filmBrightness")?),
            "r_filmdesaturation" => desaturation = Some(scalar(value, "r_filmDesaturation")?),
            "r_filmdesaturationdark" => {
                desaturation_dark = Some(scalar(value, "r_filmDesaturationDark")?)
            }
            "r_filminvert" => invert = Some(scalar(value, "r_filmInvert")? != 0.0),
            "r_filmlighttint" => light_tint = Some(vector(value, "r_filmLightTint")?),
            "r_filmmediumtint" => medium_tint = Some(vector(value, "r_filmMediumTint")?),
            "r_filmdarktint" => dark_tint = Some(vector(value, "r_filmDarkTint")?),
            "r_glow" => glow_enable = Some(scalar(value, "r_glow")? != 0.0),
            "r_glowradius0" => glow_radius = Some(scalar(value, "r_glowRadius0")?),
            "r_glowbloomcutoff" => glow_bloom_cutoff = Some(scalar(value, "r_glowBloomCutoff")?),
            "r_glowbloomdesaturation" => {
                glow_bloom_desaturation = Some(scalar(value, "r_glowBloomDesaturation")?)
            }
            "r_glowbloomintensity0" => {
                glow_bloom_intensity = Some(scalar(value, "r_glowBloomIntensity0")?)
            }
            _ => {}
        }
    }
    let missing = |key| FilmVisionParseError::MissingField(key);

    let glow = match (
        glow_enable,
        glow_radius,
        glow_bloom_cutoff,
        glow_bloom_desaturation,
        glow_bloom_intensity,
    ) {
        (Some(enable), Some(radius), Some(cutoff), Some(desaturation), Some(intensity)) => {
            (enable, radius, cutoff, desaturation, intensity)
        }
        _ => (false, 0.0, 0.0, 0.0, 0.0),
    };
    let desaturation = desaturation.ok_or_else(|| missing("r_filmDesaturation"))?;
    let light_tint = light_tint.ok_or_else(|| missing("r_filmLightTint"))?;
    let dark_tint = dark_tint.ok_or_else(|| missing("r_filmDarkTint"))?;
    Ok(Some(FilmVision {
        enable: enable.ok_or_else(|| missing("r_filmEnable"))?,
        contrast: contrast.ok_or_else(|| missing("r_filmContrast"))?,
        brightness: brightness.ok_or_else(|| missing("r_filmBrightness"))?,
        desaturation,
        desaturation_dark: desaturation_dark.unwrap_or(desaturation),
        invert: invert.ok_or_else(|| missing("r_filmInvert"))?,
        light_tint,
        medium_tint: medium_tint
            .unwrap_or_else(|| std::array::from_fn(|i| (light_tint[i] + dark_tint[i]) * 0.5)),
        dark_tint,
        glow_enable: glow.0,
        glow_radius: glow.1,
        glow_bloom_cutoff: glow.2,
        glow_bloom_desaturation: glow.3,
        glow_bloom_intensity: glow.4,
    }))
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct T6FilmGrade {
    pub controls: [[f32; 4]; 14],
}

fn vector4(value: &str) -> Option<[f32; 4]> {
    let values: Vec<f32> = value
        .trim()
        .trim_matches('"')
        .split_whitespace()
        .map(str::parse)
        .collect::<Result<_, _>>()
        .ok()?;
    values.try_into().ok()
}

pub fn parse_t6_film_grade(source: &str) -> Option<T6FilmGrade> {
    let mut enable = false;
    let mut fields = std::collections::HashMap::new();
    for line in source.lines().map(str::trim) {
        let Some(split) = line.find(char::is_whitespace) else {
            continue;
        };
        let key = line[..split].to_ascii_lowercase();
        let value = line[split..].trim();
        if key == "r_filmenable" {
            enable = value
                .trim_matches('"')
                .parse::<f32>()
                .is_ok_and(|v| v != 0.0);
        } else if let Some(name) = key.strip_prefix("vc_") {
            fields.insert(name.to_owned(), vector4(value)?);
        }
    }
    if !enable {
        return None;
    }
    let field = |name: &str| fields.get(name).copied();
    let (mut rs, mut re) = (field("rs")?, field("re")?);
    const EPSILON: f32 = 1.0 / 4096.0;
    if re[0] <= rs[0] {
        re[0] = rs[0] + EPSILON;
    }
    if re[1] <= rs[1] {
        rs[1] = re[1] - EPSILON;
    }
    if re[2] <= rs[2] {
        rs[2] = re[2] - EPSILON;
    }
    if rs[3] <= re[2] {
        rs[3] = re[2] + EPSILON;
    }
    if re[3] <= rs[3] {
        re[3] = rs[3] + EPSILON;
    }
    let scale = [
        1.0 / (rs[0] - re[0]),
        1.0 / (re[1] - rs[1]),
        1.0 / (re[2] - rs[2]),
        1.0 / (rs[3] - re[3]),
    ];
    let bias = [
        -scale[0] * re[0],
        -scale[1] * rs[1],
        -scale[2] * rs[2],
        -scale[3] * re[3],
    ];
    let fsm = field("fsm")?;
    let weight = 1.0 / (fsm[0] + fsm[1] + fsm[2]);
    Some(T6FilmGrade {
        controls: [
            scale,
            bias,
            field("smr")?,
            field("mmr")?,
            field("hmr")?,
            field("smg")?,
            field("mmg")?,
            field("hmg")?,
            field("smb")?,
            field("mmb")?,
            field("hmb")?,
            field("fgm")?,
            [fsm[0] * weight, fsm[1] * weight, fsm[2] * weight, fsm[3]],
            field("fbm")?,
        ],
    })
}
