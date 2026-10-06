use super::*;

pub(super) fn zone_ptr_kind(p: Option<ZonePtr>) -> Option<&'static str> {
    Some(match p? {
        ZonePtr::Null => "null",
        ZonePtr::Offset(_) => "offset",
        ZonePtr::Following => "following",
        ZonePtr::Insert => "insert",
    })
}

pub(super) fn zone_ptr_label(p: Option<ZonePtr>) -> Option<String> {
    match p? {
        ZonePtr::Offset(q) => Some(format!("{}:{}", q.block, q.offset)),
        _ => None,
    }
}

pub(super) fn file_key(p: Ptr) -> (u8, u32) {
    (p.block, p.offset)
}

struct InspectedSoundFile {
    file_type: Option<u8>,
    file_exists: Option<u8>,
    loaded_name: Option<String>,
    streamed: Option<(String, String)>,
    file_name: Option<String>,
}

fn inspect_sound_file(s: &ZoneStream<'_>, file: Ptr) -> InspectedSoundFile {
    let file_type = s.u8_at(file, 0).ok();
    let file_exists = s.u8_at(file, 1).ok();
    match file_type {
        Some(1) => {
            let loaded_name = match s.ptr_at(file, s.layout(4, 8)).ok() {
                Some(ZonePtr::Offset(p)) => {
                    let target = s.resolve_alias(p);

                    match s.u32_at(target, 0).ok().map(ZonePtr::decode) {
                        Some(ZonePtr::Following) | Some(ZonePtr::Insert) => None,
                        _ => name_at(s, target, 0),
                    }
                }
                _ => None,
            };
            InspectedSoundFile {
                file_type,
                file_exists,
                file_name: loaded_name.clone(),
                loaded_name,
                streamed: None,
            }
        }
        Some(2) | Some(3) => {
            let dir = name_at(s, file, s.layout(4, 8)).unwrap_or_default();
            let name = name_at(s, file, s.layout(8, 16)).unwrap_or_default();
            let file_name = if dir.is_empty() {
                name.clone()
            } else {
                format!("{dir}/{name}")
            };
            let streamed = (!name.is_empty()).then_some((dir, name));
            InspectedSoundFile {
                file_type,
                file_exists,
                loaded_name: None,
                streamed,
                file_name: (!file_name.is_empty()).then_some(file_name),
            }
        }
        _ => InspectedSoundFile {
            file_type,
            file_exists,
            loaded_name: None,
            streamed: None,
            file_name: None,
        },
    }
}

pub(super) fn name_at(s: &ZoneStream<'_>, parent: Ptr, field: usize) -> Option<String> {
    match s.ptr_at(parent, field).ok()? {
        ZonePtr::Offset(p) => s.cstr(s.resolve_alias(p)).ok().map(str::to_owned),
        ZonePtr::Null => Some(String::new()),
        _ => None,
    }
}

pub(super) fn optional_name(s: &ZoneStream<'_>, parent: Ptr, field: usize) -> Option<String> {
    name_at(s, parent, field).filter(|n| !n.is_empty())
}

pub(super) fn speaker_map_name(s: &ZoneStream<'_>, row: Ptr) -> Option<String> {
    match s.ptr_at(row, s.layout(SND_ALIAS_SPEAKER_MAP, 128)).ok()? {
        ZonePtr::Offset(p) => optional_name(s, s.resolve_alias(p), s.layout(4, 8)),
        _ => None,
    }
}

pub(super) fn stereo_speaker_gains(s: &ZoneStream<'_>, row: Ptr) -> Option<[[f32; 2]; 2]> {
    let ZonePtr::Offset(map) = s.ptr_at(row, s.layout(SND_ALIAS_SPEAKER_MAP, 128)).ok()? else {
        return None;
    };
    let map = s.resolve_alias(map);
    match s.wire_format() {
        fastfile_iw4::Iw4WireFormat::X86 => {
            capture_stereo_speaker_gains(|offset| s.u32_at(map, offset).ok())
        }
        fastfile_iw4::Iw4WireFormat::X64 => {
            let mut gains = [[0.0; 2]; 2];
            for (source, outputs) in gains.iter_mut().enumerate() {
                let channels = 16 + source * 32;
                let count = usize::from(s.u8_at(map, channels).ok()?);
                if count == 0 || count > (source + 1) * 2 {
                    return None;
                }
                let ZonePtr::Offset(entries) = s.ptr_at(map, channels + 8).ok()? else {
                    return None;
                };
                let entries = s.resolve_alias(entries);
                let mut seen = [[false; 2]; 2];
                for route in 0..count {
                    let entry = route * 8;
                    let input = usize::from(s.u8_at(entries, entry).ok()?);
                    let output = usize::from(s.u8_at(entries, entry + 1).ok()?);
                    let gain = s.f32_at(entries, entry + 4).ok()?;
                    if input > source
                        || output >= 2
                        || seen[input][output]
                        || !gain.is_finite()
                        || gain < 0.0
                    {
                        return None;
                    }
                    seen[input][output] = true;
                    if source == 0 || input == output {
                        outputs[output] = gain;
                    } else if gain != 0.0 {
                        return None;
                    }
                }
            }
            Some(gains)
        }
    }
}

pub(crate) fn capture_stereo_speaker_gains(
    mut word: impl FnMut(usize) -> Option<u32>,
) -> Option<[[f32; 2]; 2]> {
    let mut gains = [[0.0; 2]; 2];
    for (source, outputs) in gains.iter_mut().enumerate() {
        let channels = 8 + source * 200;
        if word(channels)? != 2 {
            return None;
        }
        let mut seen = [false; 2];
        for speaker in 0..2 {
            let entry = channels + 4 + speaker * 16;
            let output = usize::try_from(word(entry)?).ok()?;
            if output >= 2 || seen[output] {
                return None;
            }
            let levels = word(entry + 4)?;
            if !(1..=2).contains(&levels) {
                return None;
            }
            let left = f32::from_bits(word(entry + 8)?);
            let right = f32::from_bits(word(entry + 12)?);
            if !left.is_finite() || !right.is_finite() || left < 0.0 || right < 0.0 {
                return None;
            }
            outputs[output] = left.max(right);
            seen[output] = true;
        }
    }
    Some(gains)
}

pub(super) fn read_curve_header(s: &ZoneStream<'_>, header: Ptr) -> Option<CapturedSndCurve> {
    let name = name_at(s, header, 0).filter(|n| !n.is_empty())?;
    let count = s.u16_at(header, s.layout(SND_CURVE_KNOT_COUNT, 8)).ok()? as usize;
    let n = count.min(SND_CURVE_MAX_KNOTS);
    let mut knots = Vec::with_capacity(n);
    for i in 0..n {
        let off = s.layout(SND_CURVE_KNOTS, 12) + i * SND_CURVE_KNOT_STRIDE;
        let x = s.f32_at(header, off).ok()?;
        let y = s.f32_at(header, off + 4).ok()?;
        knots.push((x, y));
    }
    Some(CapturedSndCurve { name, knots })
}

impl AssetLinkSink for SoundCatalog {
    fn loaded(
        &mut self,
        _s: &ZoneStream<'_>,
        ty: AssetType,
        slot: Ptr,
        insert_slot: Option<Ptr>,
    ) -> Result<()> {
        match ty {
            AssetType::LoadedSound => {
                let Some(name) = self.last_loaded_name.clone() else {
                    return Ok(());
                };
                if let Some(ins) = insert_slot {
                    self.loaded_by_insert.insert(file_key(ins), name);
                }
            }
            AssetType::SoundCurve => {
                let Some(name) = self.last_curve_name.clone() else {
                    return Ok(());
                };
                self.curve_by_ptr.insert(file_key(slot), name.clone());
                if let Some(ins) = insert_slot {
                    self.curve_by_ptr.insert(file_key(ins), name);
                }
            }
            _ => {}
        }
        Ok(())
    }

    fn alias(&mut self, ty: AssetType, slot: Ptr, target: Ptr) -> Result<()> {
        if ty == AssetType::SoundCurve
            && let Some(name) = self.curve_by_ptr.get(&file_key(target)).cloned()
        {
            self.curve_by_ptr.insert(file_key(slot), name);
        }
        Ok(())
    }

    fn capture_loaded_sound(
        &mut self,
        s: &ZoneStream<'_>,
        header: Ptr,
        pcm: Ptr,
        data_len: usize,
    ) -> Result<()> {
        let name = name_at(s, header, 0).unwrap_or_default();

        let (format, rate, bits, channels, samples, block_size) =
            if s.wire_format() == fastfile_iw4::Iw4WireFormat::X64 {
                let format = i32::from(s.u16_at(header, 8)?);
                let channels = i32::from(s.u16_at(header, 10)?);
                let rate = s.u32_at(header, 12)?;
                let block_size = u32::from(s.u16_at(header, 20)?);
                let bits = i32::from(s.u16_at(header, 22)?);
                let samples = if format == MSS_PCM && block_size != 0 {
                    (data_len / block_size as usize) as u32
                } else {
                    0
                };
                (format, rate, bits, channels, samples, block_size)
            } else {
                (
                    s.i32_at(header, 4)?,
                    s.u32_at(header, 16)?,
                    s.i32_at(header, 20)?,
                    s.i32_at(header, 24)?,
                    s.u32_at(header, 28)?,
                    s.u32_at(header, 32)?,
                )
            };
        let pcm_bytes = s.slice_at(pcm, 0, data_len)?.to_vec();

        if name.is_empty() {
            self.capture_gaps += 1;
            self.last_loaded_name = None;
            return Ok(());
        }
        self.last_loaded_name = Some(name.clone());
        if pcm_bytes.is_empty() {
            if !is_null_sound_name(Some(&name)) {
                self.capture_gaps += 1;
            }
            return Ok(());
        }
        self.register_loaded(LoadedSoundPcm {
            name,
            game: self
                .capture_game
                .expect("asset capture requires an explicit family"),
            format,
            rate,
            bits,
            channels,
            samples,
            block_size,
            pcm: pcm_bytes.into(),
            zone: self.capture_zone,
            seek_table: Vec::new(),
            sab_media: None,
        });
        Ok(())
    }

    fn bind_last_loaded_to_sound_file(&mut self, file: Ptr) -> Result<()> {
        if let Some(name) = self.last_loaded_name.clone() {
            self.file_to_loaded.insert(file_key(file), name);
        } else {
            self.capture_gaps += 1;
        }
        Ok(())
    }

    fn bind_streamed_sound_file(&mut self, file: Ptr, dir: &str, name: &str) -> Result<()> {
        self.file_to_streamed
            .insert(file_key(file), (dir.to_owned(), name.to_owned()));
        Ok(())
    }

    fn capture_snd_curve(&mut self, s: &ZoneStream<'_>, header: Ptr) -> Result<()> {
        let name = name_at(s, header, 0).unwrap_or_default();
        if name.is_empty() {
            self.capture_gaps += 1;
        }
        let count = s
            .u16_at(header, s.layout(SND_CURVE_KNOT_COUNT, 8))
            .unwrap_or(0) as usize;
        if count > SND_CURVE_MAX_KNOTS {
            self.capture_gaps += 1;
        }
        let n = count.min(SND_CURVE_MAX_KNOTS);
        let mut knots = Vec::with_capacity(n);
        for i in 0..n {
            let off = s.layout(SND_CURVE_KNOTS, 12) + i * SND_CURVE_KNOT_STRIDE;
            let x = s.f32_at(header, off).unwrap_or(0.0);
            let y = s.f32_at(header, off + 4).unwrap_or(0.0);
            knots.push((x, y));
        }
        self.last_curve_name = Some(name.clone());
        self.curve_by_ptr.insert(file_key(header), name.clone());
        self.insert_curve(
            ns_of(
                self.capture_game
                    .expect("asset capture requires an explicit family"),
            ),
            CapturedSndCurve { name, knots },
        );
        Ok(())
    }

    fn capture_raw_file(&mut self, name: &str, data: &[u8], zlib_compressed: bool) -> Result<()> {
        self.ingest_rawfile(name, data, zlib_compressed);
        Ok(())
    }

    fn capture_sound(
        &mut self,
        s: &ZoneStream<'_>,
        list: Ptr,
        count: usize,
        head: Option<Ptr>,
    ) -> Result<()> {
        let name = name_at(s, list, 0).unwrap_or_default();
        let Some(arr) = head else {
            if count > 0 {
                self.capture_gaps += 1;
            }
            return Ok(());
        };

        let mut aliases = Vec::with_capacity(count);
        for i in 0..count {
            let row = arr.at(i * s.layout(sz::SND_ALIAS, 136));
            let alias_name = name_at(s, row, s.layout(SND_ALIAS_ALIAS_NAME, 0)).unwrap_or_default();
            let subtitle = optional_name(s, row, s.layout(SND_ALIAS_SUBTITLE, 8));
            let secondary = optional_name(s, row, s.layout(SND_ALIAS_SECONDARY, 16));
            let chain = optional_name(s, row, s.layout(SND_ALIAS_CHAIN, 24));
            let mixer_group = optional_name(s, row, s.layout(SND_ALIAS_MIXER_GROUP, 32));
            let (
                loaded_name,
                streamed,
                file_type,
                file_exists,
                file_name,
                file_u,
                file_u_ptr,
                file_u_deref,
            ) = match s.ptr_at(row, s.layout(SND_ALIAS_SOUND_FILE, 40)).ok() {
                Some(ZonePtr::Offset(p)) => {
                    let file = s.resolve_alias(p);
                    let key = file_key(file);
                    let loaded = self.file_to_loaded.get(&key).cloned();
                    let streamed = self.file_to_streamed.get(&key).cloned();
                    let inspected = inspect_sound_file(s, file);
                    let u = s.ptr_at(file, s.layout(4, 8)).ok();
                    let loaded = loaded.or_else(|| match u {
                        Some(ZonePtr::Offset(q)) => self.loaded_name_for_offset(s, q),
                        _ => None,
                    });
                    let file_u_deref = match u {
                        Some(ZonePtr::Offset(q)) => Self::offset_deref_kind(s, q),
                        _ => None,
                    };

                    let loaded = if file_u_deref == Some("following")
                        || file_u_deref == Some("insert")
                        || matches!(u, Some(ZonePtr::Following) | Some(ZonePtr::Insert))
                    {
                        loaded
                    } else {
                        loaded.or(inspected.loaded_name)
                    };
                    let streamed = streamed.or(inspected.streamed);
                    let file_name = loaded.clone().or(inspected.file_name.clone()).or_else(|| {
                        streamed.as_ref().map(|(dir, name)| {
                            if dir.is_empty() {
                                name.clone()
                            } else {
                                format!("{dir}/{name}")
                            }
                        })
                    });
                    (
                        loaded,
                        streamed,
                        inspected.file_type,
                        inspected.file_exists,
                        file_name,
                        zone_ptr_kind(u),
                        zone_ptr_label(u),
                        file_u_deref,
                    )
                }
                Some(ZonePtr::Null) => (None, None, None, None, None, Some("null"), None, None),
                _ => (
                    None,
                    None,
                    Some(0),
                    None,
                    None,
                    zone_ptr_kind(s.ptr_at(row, s.layout(SND_ALIAS_SOUND_FILE, 40)).ok()),
                    zone_ptr_label(s.ptr_at(row, s.layout(SND_ALIAS_SOUND_FILE, 40)).ok()),
                    None,
                ),
            };
            let flags = s.u32_at(row, s.layout(SND_ALIAS_FLAGS, 80)).ok();
            if flags.is_none() {
                self.alias_flags_missing += 1;
            }
            let volume_falloff = self.curve_for_row(s, row);
            let sound_file = s.ptr_at(row, s.layout(SND_ALIAS_SOUND_FILE, 40)).ok();
            let loaded = capture_loaded_edge(
                sound_file,
                file_type,
                file_u,
                file_u_deref,
                loaded_name.as_deref(),
            );
            aliases.push(CapturedAlias {
                alias_name,
                subtitle,
                secondary,
                chain,
                mixer_group,
                loaded_name,
                loaded,
                loaded_binding_origin: crate::LoadedBindingOrigin::Unresolved,
                streamed,
                file_type,
                file_exists,
                file_name,
                file_u,
                file_u_ptr,
                file_u_deref,
                sequence: s.i32_at(row, s.layout(SND_ALIAS_SEQUENCE, 48)).unwrap_or(0),
                vol_min: s
                    .f32_at(row, s.layout(SND_ALIAS_VOL_MIN, 52))
                    .unwrap_or(0.0),
                vol_max: s
                    .f32_at(row, s.layout(SND_ALIAS_VOL_MAX, 56))
                    .unwrap_or(0.0),
                vol_mod_index: None,
                pitch_min: s
                    .f32_at(row, s.layout(SND_ALIAS_PITCH_MIN, 60))
                    .unwrap_or(0.0),
                pitch_max: s
                    .f32_at(row, s.layout(SND_ALIAS_PITCH_MAX, 64))
                    .unwrap_or(0.0),
                dist_min: s
                    .f32_at(row, s.layout(SND_ALIAS_DIST_MIN, 68))
                    .unwrap_or(0.0),
                dist_max: s
                    .f32_at(row, s.layout(SND_ALIAS_DIST_MAX, 72))
                    .unwrap_or(0.0),
                velocity_min: s
                    .f32_at(row, s.layout(SND_ALIAS_VELOCITY_MIN, 76))
                    .unwrap_or(0.0),
                flags,
                looping: None,
                slave_percentage: s
                    .f32_at(row, s.layout(SND_ALIAS_SLAVE_PERCENTAGE, 84))
                    .unwrap_or(0.0),
                probability: s
                    .f32_at(row, s.layout(SND_ALIAS_PROBABILITY, 88))
                    .unwrap_or(1.0),
                lfe_percentage: s
                    .f32_at(row, s.layout(SND_ALIAS_LFE_PERCENTAGE, 92))
                    .unwrap_or(0.0),
                center_percentage: s
                    .f32_at(row, s.layout(SND_ALIAS_CENTER_PERCENTAGE, 96))
                    .unwrap_or(0.0),
                start_delay: s
                    .i32_at(row, s.layout(SND_ALIAS_START_DELAY, 100))
                    .unwrap_or(0),
                volume_falloff,
                t5_distance_curves: None,
                near_falloff: None,
                voice_priority: None,
                envelop_min: s
                    .f32_at(row, s.layout(SND_ALIAS_ENVELOP_MIN, 112))
                    .unwrap_or(0.0),
                envelop_max: s
                    .f32_at(row, s.layout(SND_ALIAS_ENVELOP_MAX, 116))
                    .unwrap_or(0.0),
                envelop_percentage: s
                    .f32_at(row, s.layout(SND_ALIAS_ENVELOP_PERCENTAGE, 120))
                    .unwrap_or(0.0),
                speaker_map: speaker_map_name(s, row),
                stereo_speaker_gains: stereo_speaker_gains(s, row),
                limit_count: None,
                entity_limit_count: None,
            });
        }

        if name.is_empty() {
            self.capture_gaps += 1;
            return Ok(());
        }
        self.ingest_sound(CapturedSound {
            name,
            aliases,
            game: self
                .capture_game
                .expect("asset capture requires an explicit family"),
            zone: self.capture_zone,
        });
        Ok(())
    }
}
