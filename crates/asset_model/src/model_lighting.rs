use bevy::tasks::{ComputeTaskPool, TaskPool};
use fastfile_iw4::{GfxLightGridGeometry, GfxWorldGeometry, ZoneStream};
use lighting_iw4::{
    GFX_STATIC_MODEL_INST_SIZE, GfxLightGridEntry, LIGHT_GRID_ATPOINT_EMPTY_PRIMARY,
    LIGHT_GRID_COLORS_BYTE_COUNT, LIGHT_GRID_COMPRESS_DIRS, LIGHT_GRID_ROW_ABSENT,
    LIGHT_GRID_ROW_HEADER_SIZE, LightGridAtPointEmptyGate, LightGridAtPointPath,
    LightGridEntryQuad, LightGridLookupCorner, LightGridPickCorner, LightGridRowHeader,
    MODEL_LIGHTING_TILE_BYTES, decode_model_lighting_sample, light_grid_accumulate_colors,
    light_grid_at_row_last_column, light_grid_atapoint_return_primary,
    light_grid_atapoint_select_path, light_grid_axis_lerps,
    light_grid_column_before_row_start_quad, light_grid_compress_colors,
    light_grid_corner_needs_trace, light_grid_corner_weight_keeps_entry, light_grid_corner_weights,
    light_grid_default_colors_index, light_grid_empty_run_entry_quad,
    light_grid_expand_shell_to_tile_rgba, light_grid_fixed_point_blend_weights,
    light_grid_isvalid_sample, light_grid_lookup_accumulate_corners,
    light_grid_lookup_remap_primary, light_grid_pick_default_grid_entry,
    light_grid_pick_merge_entry_quads, light_grid_pick_primary_from_corners,
    light_grid_pick_row_rle_cells, light_grid_primary_light_sun_band,
    light_grid_rle_advance_to_column, light_grid_row_contains_cell, light_grid_sample_cell,
    light_grid_solid_run_entry_quad, light_grid_solid_run_z_base, lighting_origin_from_inst_bytes,
    lit_albedo, lit_fragment_color, lit_sun_lighting, model_lighting_local_tile_nearest_index,
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LightGridColorEncoding {
    Rgb8,
    T5Y12U6W6,
    T6Coefficients,
}

#[derive(Clone, Debug)]
pub struct OwnedLightGrid {
    pub mins: [u16; 3],
    pub maxs: [u16; 3],
    pub row_axis: usize,
    pub col_axis: usize,
    pub color_count: u32,

    pub has_light_regions: bool,

    pub sun_primary_light_index: u32,
    pub row_data_start: Vec<u8>,
    pub raw_row_data: Vec<u8>,
    pub entries: Vec<u8>,
    pub colors: Vec<u8>,
    pub color_encoding: LightGridColorEncoding,
}

impl OwnedLightGrid {
    pub fn view(&self) -> GridView<'_> {
        GridView {
            mins: self.mins,
            maxs: self.maxs,
            row_axis: self.row_axis,
            col_axis: self.col_axis,
            has_light_regions: self.has_light_regions,
            sun_primary_light_index: self.sun_primary_light_index,
            row_data_start: &self.row_data_start,
            raw_row_data: &self.raw_row_data,
            entries: &self.entries,
            colors: &self.colors,
            color_encoding: self.color_encoding,
            color_count: self.color_count,
        }
    }

    pub fn from_stream(stream: &ZoneStream<'_>, grid: GfxLightGridGeometry) -> Option<Self> {
        if grid.row_axis > 2 || grid.col_axis > 2 {
            return None;
        }
        let (Some(row_data_start), Some(raw_row_data), Some(entries), Some(colors)) = (
            grid.row_data_start,
            grid.raw_row_data,
            grid.entries,
            grid.colors,
        ) else {
            return None;
        };
        let row_data_start = stream
            .slice_at(row_data_start, 0, grid.row_count * 2)
            .ok()?
            .to_vec();
        let raw_row_data = stream
            .slice_at(raw_row_data, 0, grid.raw_row_data_size)
            .ok()?
            .to_vec();
        let entries = stream
            .slice_at(entries, 0, grid.entry_count * 4)
            .ok()?
            .to_vec();
        let colors = stream
            .slice_at(colors, 0, grid.color_count * LIGHT_GRID_COLORS_BYTE_COUNT)
            .ok()?
            .to_vec();
        Some(Self {
            mins: grid.mins,
            maxs: grid.maxs,
            row_axis: grid.row_axis as usize,
            col_axis: grid.col_axis as usize,
            color_count: grid.color_count as u32,
            has_light_regions: grid.has_light_regions,
            sun_primary_light_index: grid.sun_primary_light_index,
            row_data_start,
            raw_row_data,
            entries,
            colors,
            color_encoding: LightGridColorEncoding::Rgb8,
        })
    }

    pub fn from_iw5_stream(
        stream: &fastfile_iw5::ZoneStream<'_>,
        grid: fastfile_iw5::GfxLightGridGeometry,
    ) -> Option<Self> {
        if grid.row_axis > 2 || grid.col_axis > 2 {
            return None;
        }
        let (Some(row_data_start), Some(raw_row_data), Some(entries), Some(colors)) = (
            grid.row_data_start,
            grid.raw_row_data,
            grid.entries,
            grid.colors,
        ) else {
            return None;
        };
        let row_data_start = stream
            .slice_at(row_data_start, 0, grid.row_count * 2)
            .ok()?
            .to_vec();
        let raw_row_data = stream
            .slice_at(raw_row_data, 0, grid.raw_row_data_size)
            .ok()?
            .to_vec();
        let entries = stream
            .slice_at(entries, 0, grid.entry_count * 4)
            .ok()?
            .to_vec();
        let colors = stream
            .slice_at(colors, 0, grid.color_count * LIGHT_GRID_COLORS_BYTE_COUNT)
            .ok()?
            .to_vec();
        Some(Self {
            mins: grid.mins,
            maxs: grid.maxs,
            row_axis: grid.row_axis as usize,
            col_axis: grid.col_axis as usize,
            color_count: grid.color_count as u32,
            has_light_regions: grid.has_light_regions,
            sun_primary_light_index: grid.sun_primary_light_index,
            row_data_start,
            raw_row_data,
            entries,
            colors,
            color_encoding: LightGridColorEncoding::Rgb8,
        })
    }

    pub fn from_t5_stream(
        stream: &fastfile_t5::ZoneStream<'_>,
        grid: fastfile_t5::GfxLightGridGeometry,
    ) -> Option<Self> {
        if grid.row_axis > 2 || grid.col_axis > 2 {
            return None;
        }
        let (Some(row_data_start), Some(raw_row_data), Some(entries), Some(colors)) = (
            grid.row_data_start,
            grid.raw_row_data,
            grid.entries,
            grid.colors,
        ) else {
            return None;
        };
        let row_data_start = stream
            .slice_at(row_data_start, 0, grid.row_count * 2)
            .ok()?
            .to_vec();
        let raw_row_data = stream
            .slice_at(raw_row_data, 0, grid.raw_row_data_size)
            .ok()?
            .to_vec();
        let entries = stream
            .slice_at(entries, 0, grid.entry_count * 4)
            .ok()?
            .to_vec();
        let colors = stream
            .slice_at(colors, 0, grid.color_count * LIGHT_GRID_COLORS_BYTE_COUNT)
            .ok()?
            .to_vec();
        Some(Self {
            mins: grid.mins,
            maxs: grid.maxs,
            row_axis: grid.row_axis as usize,
            col_axis: grid.col_axis as usize,
            color_count: grid.color_count as u32,
            has_light_regions: grid.has_light_regions,
            sun_primary_light_index: grid.sun_primary_light_index,
            row_data_start,
            raw_row_data,
            entries,
            colors,
            color_encoding: LightGridColorEncoding::T5Y12U6W6,
        })
    }
}

#[derive(Clone, Copy, Debug)]
pub struct GridView<'a> {
    pub mins: [u16; 3],
    pub maxs: [u16; 3],
    pub row_axis: usize,
    pub col_axis: usize,

    pub has_light_regions: bool,

    pub sun_primary_light_index: u32,

    pub row_data_start: &'a [u8],
    pub raw_row_data: &'a [u8],

    pub entries: &'a [u8],

    pub colors: &'a [u8],
    pub color_encoding: LightGridColorEncoding,
    pub color_count: u32,
}

impl GridView<'_> {
    pub fn entry_at(&self, byte_offset: u32) -> Option<GfxLightGridEntry> {
        let start = byte_offset as usize;
        let bytes = self.entries.get(start..start.checked_add(4)?)?;
        Some(GfxLightGridEntry {
            colors_index: u16::from_le_bytes([bytes[0], bytes[1]]),
            primary_light_index: bytes[2],
            needs_trace: bytes[3],
        })
    }

    pub fn colors_row(&self, index: u32) -> Option<&[u8]> {
        if index >= self.color_count {
            return None;
        }
        let stride = match self.color_encoding {
            LightGridColorEncoding::T6Coefficients => {
                fastfile_t6::light_grid::COEFFICIENT_ROW_BYTES
            }
            _ => LIGHT_GRID_COLORS_BYTE_COUNT,
        };
        let start = (index as usize).checked_mul(stride)?;
        self.colors.get(start..start.checked_add(stride)?)
    }
}

pub fn grid_view_from_geometry<'a>(
    stream: &'a ZoneStream<'_>,
    grid: GfxLightGridGeometry,
) -> Option<GridView<'a>> {
    if grid.row_axis > 2 || grid.col_axis > 2 {
        return None;
    }
    let (Some(row_data_start), Some(raw_row_data), Some(entries), Some(colors)) = (
        grid.row_data_start,
        grid.raw_row_data,
        grid.entries,
        grid.colors,
    ) else {
        return None;
    };
    Some(GridView {
        mins: grid.mins,
        maxs: grid.maxs,
        row_axis: grid.row_axis as usize,
        col_axis: grid.col_axis as usize,
        has_light_regions: grid.has_light_regions,
        sun_primary_light_index: grid.sun_primary_light_index,
        row_data_start: stream
            .slice_at(row_data_start, 0, grid.row_count * 2)
            .ok()?,
        raw_row_data: stream
            .slice_at(raw_row_data, 0, grid.raw_row_data_size)
            .ok()?,
        entries: stream.slice_at(entries, 0, grid.entry_count * 4).ok()?,
        color_encoding: LightGridColorEncoding::Rgb8,
        colors: stream
            .slice_at(colors, 0, grid.color_count * LIGHT_GRID_COLORS_BYTE_COUNT)
            .ok()?,
        color_count: grid.color_count as u32,
    })
}

enum RowRleOutcome {
    OutsideGrid,
    OutsideRow { clear_needs_trace: bool },
    Truncated,
    Quad(LightGridEntryQuad),
}

impl RowRleOutcome {
    fn quad(&self) -> Option<LightGridEntryQuad> {
        match self {
            RowRleOutcome::Quad(quad) => Some(*quad),
            RowRleOutcome::OutsideGrid => Some(LightGridEntryQuad {
                corners: [None; 4],
                clear_needs_trace: false,
            }),
            RowRleOutcome::OutsideRow { clear_needs_trace } => Some(LightGridEntryQuad {
                corners: [None; 4],
                clear_needs_trace: *clear_needs_trace,
            }),
            RowRleOutcome::Truncated => None,
        }
    }
}

fn row_rle(grid: &GridView<'_>, cell: [i32; 3]) -> RowRleOutcome {
    let min = u32::from(grid.mins[grid.row_axis]);
    let max = u32::from(grid.maxs[grid.row_axis]);
    let row = (cell[grid.row_axis] as u32).wrapping_sub(min);
    if max.wrapping_sub(min).wrapping_add(1) <= row {
        return RowRleOutcome::OutsideGrid;
    }

    let word_at = (row as usize).wrapping_mul(2);
    let Some(word) = grid.row_data_start.get(word_at..word_at.wrapping_add(2)) else {
        return RowRleOutcome::Truncated;
    };
    let row_start_dwords = u16::from_le_bytes([word[0], word[1]]);
    if row_start_dwords == LIGHT_GRID_ROW_ABSENT {
        return RowRleOutcome::OutsideGrid;
    }

    let Some(payload) = grid
        .raw_row_data
        .get(usize::from(row_start_dwords).wrapping_mul(4)..)
    else {
        return RowRleOutcome::Truncated;
    };
    let Some(header) = LightGridRowHeader::from_bytes(payload) else {
        return RowRleOutcome::Truncated;
    };

    let col = (cell[grid.col_axis] as u32).wrapping_sub(u32::from(header.col_start));
    let row_in_header = (cell[2] as u32).wrapping_sub(u32::from(header.row_start));
    if !light_grid_row_contains_cell(&header, col, row_in_header) {
        let column_fits = u32::from(header.col_count) >= col.wrapping_add(1);
        return RowRleOutcome::OutsideRow {
            clear_needs_trace: column_fits && (cell[2] as u32) < u32::from(header.row_start),
        };
    }
    if col == u32::MAX {
        let rle = &payload[LIGHT_GRID_ROW_HEADER_SIZE..];
        let wide_row = header.row_count > 0xff;
        return match light_grid_column_before_row_start_quad(
            header.entry_base,
            row_in_header,
            rle,
            wide_row,
        ) {
            Some(quad) => RowRleOutcome::Quad(quad),
            None => RowRleOutcome::Truncated,
        };
    }

    let rle = &payload[LIGHT_GRID_ROW_HEADER_SIZE..];
    let Some(cursor) =
        light_grid_rle_advance_to_column(rle, header.row_count, header.entry_base, col)
    else {
        return RowRleOutcome::Truncated;
    };

    let wide_row = header.row_count > 0xff;
    let stride = if cursor.run_increment == 0 {
        2
    } else {
        header.rle_stride()
    };
    let next_run = rle
        .get(cursor.run_offset.wrapping_add(stride)..)
        .filter(|run| !run.is_empty());
    let at_last_column = light_grid_at_row_last_column(cell[grid.col_axis] as u32, &header);

    let quad = if cursor.run_increment == 0 {
        light_grid_empty_run_entry_quad(&cursor, row_in_header, at_last_column, next_run, wide_row)
    } else {
        let run = &rle[cursor.run_offset..];
        let Some(z_base) = light_grid_solid_run_z_base(run, wide_row) else {
            return RowRleOutcome::Truncated;
        };
        light_grid_solid_run_entry_quad(
            &cursor,
            row_in_header,
            z_base,
            at_last_column,
            next_run,
            wide_row,
        )
    };
    match quad {
        Some(quad) => RowRleOutcome::Quad(quad),
        None => RowRleOutcome::Truncated,
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BlockedReason {
    UnmodelledRowBranch,

    TruncatedZoneData,

    NoLiveCorner,
}

#[derive(Clone, Debug)]
pub struct SampledLighting {
    pub live_corners: usize,
    pub weights: [f32; 8],
    pub path: LightGridAtPointPath,
    pub sample_count: u32,
    pub matching_primary: f32,
    pub traced_influence: f32,
    pub total: f32,
    pub picked_primary: u8,

    pub picked_before_remap: u8,

    pub corner_primaries: [Option<u8>; 8],

    pub needs_trace_flag: bool,

    pub corners_needing_sight: u8,

    pub corners_sight_cleared: u8,

    pub corners_sight_suppressed: u8,
    pub colors: [u8; LIGHT_GRID_COLORS_BYTE_COUNT],
    pub compressed: [u8; 3],
    pub tile: [u8; MODEL_LIGHTING_TILE_BYTES],
    pub lighting_sh: Option<[[f32; 4]; 3]>,
}

pub fn sample_light_grid(
    grid: &GridView<'_>,
    pos: [f32; 3],
) -> Result<SampledLighting, BlockedReason> {
    sample_light_grid_at(grid, pos, None, LIGHT_GRID_ATPOINT_EMPTY_PRIMARY)
}

pub fn sample_light_grid_with_sight(
    grid: &GridView<'_>,
    pos: [f32; 3],
    sight_clear: Option<&dyn Fn([f32; 3], [f32; 3]) -> bool>,
) -> Result<SampledLighting, BlockedReason> {
    sample_light_grid_at(grid, pos, sight_clear, LIGHT_GRID_ATPOINT_EMPTY_PRIMARY)
}

pub fn sample_light_grid_with_lookup_fallback(
    grid: &GridView<'_>,
    pos: [f32; 3],
    lookup_fallback: u8,
) -> Result<SampledLighting, BlockedReason> {
    sample_light_grid_at(grid, pos, None, lookup_fallback)
}

/// A sample for a world the grid does not describe: the same grey from
/// every direction, bright enough that white renders white, and no primary
/// light. Minecraft's own lighting is applied over it.
pub fn neutral_light_grid_sample(grid: &GridView<'_>) -> Result<SampledLighting, BlockedReason> {
    if grid.color_encoding != LightGridColorEncoding::Rgb8 {
        return Err(BlockedReason::TruncatedZoneData);
    }
    let shell = |grey: u8| -> Option<(
        [u8; LIGHT_GRID_COLORS_BYTE_COUNT],
        lighting_iw4::LightGridCompressedColor,
        [u8; MODEL_LIGHTING_TILE_BYTES],
    )> {
        let colors = [grey; LIGHT_GRID_COLORS_BYTE_COUNT];
        let compressed = light_grid_compress_colors(&colors, 0xff)?;
        let mut tile = [0u8; MODEL_LIGHTING_TILE_BYTES];
        light_grid_expand_shell_to_tile_rgba(&colors, compressed.weight, &mut tile)
            .then_some((colors, compressed, tile))
    };
    static WHITE: std::sync::OnceLock<u8> = std::sync::OnceLock::new();
    let grey = *WHITE.get_or_init(|| {
        (0..=255u8)
            .min_by(|&a, &b| {
                let error = |grey| {
                    shell(grey).map_or(f32::MAX, |(_, _, tile)| {
                        (lit_fragment_white_from_tile(&tile)[1] - 1.0).abs()
                    })
                };
                error(a).total_cmp(&error(b))
            })
            .unwrap_or(128)
    });
    let (colors, compressed, tile) = shell(grey).ok_or(BlockedReason::TruncatedZoneData)?;
    Ok(SampledLighting {
        live_corners: 0,
        weights: [0.0; 8],
        path: LightGridAtPointPath::SetDefault,
        sample_count: 0,
        matching_primary: 0.0,
        traced_influence: 0.0,
        total: 0.0,
        picked_primary: 0,
        picked_before_remap: 0,
        corner_primaries: [None; 8],
        needs_trace_flag: false,
        corners_needing_sight: 0,
        corners_sight_cleared: 0,
        corners_sight_suppressed: 0,
        colors,
        compressed: [compressed.r, compressed.g, compressed.b],
        tile,
        lighting_sh: None,
    })
}

fn sample_light_grid_at(
    grid: &GridView<'_>,
    pos: [f32; 3],
    sight_clear: Option<&dyn Fn([f32; 3], [f32; 3]) -> bool>,
    lookup_fallback: u8,
) -> Result<SampledLighting, BlockedReason> {
    let cell = light_grid_sample_cell(pos);
    let lerps = light_grid_axis_lerps(pos, cell, grid.row_axis as u32, grid.col_axis as u32);
    let weights = light_grid_corner_weights(lerps);

    let Some(cells) = light_grid_pick_row_rle_cells(cell, grid.row_axis as u32) else {
        return Err(BlockedReason::TruncatedZoneData);
    };
    let low = row_rle(grid, cells.base);
    let high = row_rle(grid, cells.adjacent);
    let (Some(low_quad), Some(high_quad)) = (low.quad(), high.quad()) else {
        let truncated =
            matches!(low, RowRleOutcome::Truncated) || matches!(high, RowRleOutcome::Truncated);
        return Err(if truncated {
            BlockedReason::TruncatedZoneData
        } else {
            BlockedReason::UnmodelledRowBranch
        });
    };
    let needs_trace_flag = !(low_quad.clear_needs_trace || high_quad.clear_needs_trace);
    let slots = light_grid_pick_merge_entry_quads(low_quad, high_quad);

    let mut entries: [Option<GfxLightGridEntry>; 8] = [None; 8];
    for (i, slot) in slots.iter().enumerate() {
        if let Some(offset) = *slot {
            let Some(entry) = grid.entry_at(offset) else {
                return Err(BlockedReason::TruncatedZoneData);
            };
            entries[i] = Some(entry);
        }
    }

    let t6_visibility = grid.color_encoding == LightGridColorEncoding::T6Coefficients;
    let mut corners_needing_sight = 0u8;
    let mut corners_sight_cleared = 0u8;
    let mut corners_sight_suppressed = 0u8;
    let corners: [LightGridPickCorner; 8] = core::array::from_fn(|i| {
        let entry = entries[i];
        let needs_trace = if t6_visibility {
            0
        } else {
            entry.map_or(0, |entry| entry.needs_trace)
        };
        let weight = weights[i];
        let asks = entry.is_some()
            && light_grid_corner_weight_keeps_entry(weight)
            && light_grid_corner_needs_trace(needs_trace, i as u32);
        let is_valid_if_traced = if !asks {
            false
        } else {
            corners_needing_sight = corners_needing_sight.saturating_add(1);
            let valid = match sight_clear {
                Some(clear) => light_grid_isvalid_sample(
                    cell,
                    i as u32,
                    grid.row_axis as u32,
                    grid.col_axis as u32,
                    pos,
                    clear,
                )
                .unwrap_or(false),

                None => false,
            };
            if valid {
                corners_sight_cleared = corners_sight_cleared.saturating_add(1);
            } else {
                corners_sight_suppressed = corners_sight_suppressed.saturating_add(1);
            }
            valid
        };
        LightGridPickCorner {
            primary_light: entry.map(|entry| entry.primary_light_index),
            needs_trace,
            weight,
            is_valid_if_traced,
        }
    });

    let sun_base = grid.sun_primary_light_index;
    let sun_band = light_grid_primary_light_sun_band(sun_base);
    let picked = light_grid_pick_primary_from_corners(&corners, sun_band);

    let selected_primary = light_grid_lookup_remap_primary(
        picked.primary_light,
        lookup_fallback,
        grid.has_light_regions && !t6_visibility,
        sun_base,
    );

    let live: Vec<LightGridLookupCorner> = (0..8)
        .filter(|&i| picked.entry_alive[i])
        .filter_map(|i| {
            entries[i].map(|entry| LightGridLookupCorner {
                colors_index: entry.colors_index,
                primary_light: entry.primary_light_index,
                weight: weights[i],

                trace_allows: false,
            })
        })
        .collect();

    let default_grid_entry =
        light_grid_pick_default_grid_entry(low_quad.clear_needs_trace, high_quad.clear_needs_trace);
    let (buckets, accum) = light_grid_lookup_accumulate_corners(&live, selected_primary, sun_band);
    let path = light_grid_atapoint_select_path(
        accum.count,
        LightGridAtPointEmptyGate {
            prefer_default_when_missing: false,
            fallback_colors_index: default_grid_entry,
            show_missing_light_grid: false,
            color_count: grid.color_count,
        },
    );

    let (colors, compressed, tile, lighting_sh) = if grid.color_encoding
        != LightGridColorEncoding::Rgb8
    {
        let mut rows = Vec::new();
        match path {
            LightGridAtPointPath::SetFromIndex => rows.push((u32::from(accum.indices[0]), 1.0)),
            LightGridAtPointPath::BlendAndSet => {
                if buckets.total == 0.0 {
                    return Err(BlockedReason::NoLiveCorner);
                }
                for i in 0..accum.count as usize {
                    rows.push((
                        u32::from(accum.indices[i]),
                        accum.weights[i] / buckets.total,
                    ));
                }
            }
            LightGridAtPointPath::MissingUseIndex => rows.push((default_grid_entry, 1.0)),
            LightGridAtPointPath::SetDefault => rows.push((
                light_grid_default_colors_index(grid.color_count)
                    .ok_or(BlockedReason::TruncatedZoneData)?,
                1.0,
            )),
        }
        let mut linear = [[0.0; 3]; 56];
        let mut blended_coefficients = [[0.0; 3]; 9];
        for (index, weight) in rows {
            let row = grid
                .colors_row(index)
                .ok_or(BlockedReason::TruncatedZoneData)?;
            let decoded = match grid.color_encoding {
                LightGridColorEncoding::T5Y12U6W6 => {
                    fastfile_t5::light_grid::decode_light_grid_colors(row)
                        .ok_or(BlockedReason::TruncatedZoneData)?
                }
                LightGridColorEncoding::T6Coefficients => {
                    let coefficients = fastfile_t6::light_grid::decode_coefficients(row)
                        .ok_or(BlockedReason::TruncatedZoneData)?;
                    for (out, value) in blended_coefficients.iter_mut().zip(coefficients) {
                        for c in 0..3 {
                            out[c] += value[c] * weight;
                        }
                    }
                    fastfile_t6::light_grid::directional_colors(&coefficients)
                }
                LightGridColorEncoding::Rgb8 => unreachable!(),
            };
            for (out, value) in linear.iter_mut().zip(decoded) {
                for c in 0..3 {
                    out[c] += value[c] * weight;
                }
            }
        }
        let t6 = grid.color_encoding == LightGridColorEncoding::T6Coefficients;
        let tile = linear_light_grid_tile(&linear, 0xff, t6);

        let mut colors = [0u8; LIGHT_GRID_COLORS_BYTE_COUNT];
        for (out, value) in colors.chunks_exact_mut(3).zip(linear) {
            out.copy_from_slice(&value.map(|value| linear_light_grid_channel(value, t6)));
        }
        let compressed =
            light_grid_compress_colors(&colors, 0xff).ok_or(BlockedReason::TruncatedZoneData)?;
        let lighting_sh = t6.then(|| fastfile_t6::light_grid::lighting_sh(&blended_coefficients));
        (colors, compressed, tile, lighting_sh)
    } else {
        let mut colors = [0u8; LIGHT_GRID_COLORS_BYTE_COUNT];
        match path {
            LightGridAtPointPath::SetFromIndex => {
                let Some(row) = grid.colors_row(u32::from(accum.indices[0])) else {
                    return Err(BlockedReason::TruncatedZoneData);
                };
                colors.copy_from_slice(row);
            }
            LightGridAtPointPath::BlendAndSet => {
                let count = accum.count as usize;
                let mut rows: Vec<&[u8]> = Vec::with_capacity(count);
                for &index in &accum.indices[..count] {
                    let Some(row) = grid.colors_row(u32::from(index)) else {
                        return Err(BlockedReason::TruncatedZoneData);
                    };
                    rows.push(row);
                }
                let mut fixed = [0u16; 8];
                let inverse = if buckets.total != 0.0 {
                    1.0 / buckets.total
                } else {
                    0.0
                };
                if light_grid_fixed_point_blend_weights(
                    &accum.weights[..count],
                    inverse,
                    &mut fixed,
                )
                .is_none()
                {
                    return Err(BlockedReason::NoLiveCorner);
                }
                if !light_grid_accumulate_colors(&rows, &fixed[..count], &mut colors) {
                    return Err(BlockedReason::TruncatedZoneData);
                }
            }
            LightGridAtPointPath::MissingUseIndex => {
                let Some(row) = grid.colors_row(default_grid_entry) else {
                    return Err(BlockedReason::TruncatedZoneData);
                };
                colors.copy_from_slice(row);
            }
            LightGridAtPointPath::SetDefault => {
                let Some(index) = light_grid_default_colors_index(grid.color_count) else {
                    return Err(BlockedReason::TruncatedZoneData);
                };
                let Some(row) = grid.colors_row(index) else {
                    return Err(BlockedReason::TruncatedZoneData);
                };
                colors.copy_from_slice(row);
            }
        }

        let Some(compressed) = light_grid_compress_colors(&colors, 0xff) else {
            return Err(BlockedReason::TruncatedZoneData);
        };
        let mut tile = [0u8; MODEL_LIGHTING_TILE_BYTES];
        if !light_grid_expand_shell_to_tile_rgba(&colors, compressed.weight, &mut tile) {
            return Err(BlockedReason::TruncatedZoneData);
        }
        (colors, compressed, tile, None)
    };

    Ok(SampledLighting {
        live_corners: live.len(),
        weights,
        path,
        sample_count: accum.count,
        matching_primary: buckets.matching_primary,
        traced_influence: buckets.traced_influence,
        total: buckets.total,
        picked_primary: light_grid_atapoint_return_primary(path, selected_primary),
        picked_before_remap: picked.primary_light,
        corner_primaries: core::array::from_fn(|i| corners[i].primary_light),
        needs_trace_flag,
        corners_needing_sight,
        corners_sight_cleared,
        corners_sight_suppressed,
        colors,
        compressed: [compressed.r, compressed.g, compressed.b],
        tile,
        lighting_sh,
    })
}

pub fn model_lighting_texel_decode_scale(encoding: LightGridColorEncoding) -> f32 {
    match encoding {
        LightGridColorEncoding::Rgb8 => core::f32::consts::FRAC_1_SQRT_2 / 2.0,
        LightGridColorEncoding::T5Y12U6W6 | LightGridColorEncoding::T6Coefficients => 1.0,
    }
}

fn linear_light_grid_channel(value: f32, t6: bool) -> u8 {
    if t6 {
        ((value.clamp(0.0, 32.0) * 0.03125).sqrt() * 255.0) as u8
    } else {
        ((value * f32::from_bits(0x3d008081)).min(1.0).sqrt() * 255.0).round_ties_even() as u8
    }
}

fn linear_light_grid_tile(
    linear: &[[f32; 3]; 56],
    alpha: u8,
    t6: bool,
) -> [u8; MODEL_LIGHTING_TILE_BYTES] {
    let mut ambient = [0.0; 3];
    for sample in linear {
        for c in 0..3 {
            ambient[c] += sample[c];
        }
    }
    let ambient = ambient.map(|v| v / 56.0);
    let mut tile = [0u8; MODEL_LIGHTING_TILE_BYTES];
    let mut shell = 0;
    for z in 0..4 {
        for y in 0..4 {
            for x in 0..4 {
                let interior = (1..3).contains(&x) && (1..3).contains(&y) && (1..3).contains(&z);
                let value = if interior {
                    if t6 { [0.0; 3] } else { ambient }
                } else {
                    let value = linear[shell];
                    shell += 1;
                    value
                };
                let offset = ((z * 4 + y) * 4 + x) * 4;
                tile[offset..offset + 3]
                    .copy_from_slice(&value.map(|value| linear_light_grid_channel(value, t6)));
                tile[offset + 3] = if t6 && interior { 0 } else { alpha };
            }
        }
    }
    tile
}

pub fn tile_corners_match_compress(tile: &[u8; MODEL_LIGHTING_TILE_BYTES], colors: &[u8]) -> bool {
    const CORNER_TEXELS: [usize; 8] = [0, 3, 12, 15, 48, 51, 60, 63];
    CORNER_TEXELS.iter().enumerate().all(|(i, &texel)| {
        let source = LIGHT_GRID_COMPRESS_DIRS[i] * 3;
        tile[texel * 4..texel * 4 + 3] == colors[source..source + 3]
    })
}

#[derive(Clone, Debug)]
pub struct SmodelLightingSample {
    pub authored_slot: usize,
    pub lighting_origin: [f32; 3],

    pub tile_rgba: [u8; MODEL_LIGHTING_TILE_BYTES],
    pub colors: [u8; LIGHT_GRID_COLORS_BYTE_COUNT],

    pub packed_lighting: [u8; 4],
    pub path: LightGridAtPointPath,
    pub lighting_sh: Option<[[f32; 4]; 3]>,
    pub vertex_lighting: Option<std::sync::Arc<[Vec<[u8; 4]>; 4]>>,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct SmodelLightingCensus {
    pub candidates: usize,
    pub lit: usize,
    pub blocked_unmodelled_row: usize,
    pub blocked_truncated: usize,
    pub blocked_no_live_corner: usize,

    pub corners_sight_suppressed: usize,

    pub corners_needing_sight: usize,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct LitFragmentTileCensus {
    pub tiles: usize,
    pub lum_min: f32,
    pub lum_max: f32,
    pub lum_mean: f32,
}

pub fn mean_tile_rgba01(tile: &[u8; MODEL_LIGHTING_TILE_BYTES]) -> [f32; 3] {
    let texels = MODEL_LIGHTING_TILE_BYTES / 4;
    let mut sum = [0.0f32; 3];
    for i in 0..texels {
        let o = i * 4;
        sum[0] += f32::from(tile[o]) / 255.0;
        sum[1] += f32::from(tile[o + 1]) / 255.0;
        sum[2] += f32::from(tile[o + 2]) / 255.0;
    }
    let n = texels as f32;
    [sum[0] / n, sum[1] / n, sum[2] / n]
}

pub fn lit_fragment_mid_grey_from_tile(tile: &[u8; MODEL_LIGHTING_TILE_BYTES]) -> [f32; 3] {
    let sample = mean_tile_rgba01(tile);
    let light = decode_model_lighting_sample(sample);
    let albedo = lit_albedo([0.5, 0.5, 0.5], [1.0, 1.0, 1.0]);
    lit_fragment_color(albedo, light, [0.0, 0.0, 0.0])
}

pub fn lit_fragment_white_from_tile(tile: &[u8; MODEL_LIGHTING_TILE_BYTES]) -> [f32; 3] {
    let sample = mean_tile_rgba01(tile);
    let light = decode_model_lighting_sample(sample);
    let albedo = lit_albedo([1.0, 1.0, 1.0], [1.0, 1.0, 1.0]);
    lit_fragment_color(albedo, light, [0.0, 0.0, 0.0])
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct TileChromaSpan {
    pub texels: usize,
    pub red_dominant: usize,
    pub blue_dominant: usize,
    pub min_r_minus_b: i16,
    pub max_r_minus_b: i16,
    pub alpha: u8,
}

pub fn tile_chroma_span(tile: &[u8; MODEL_LIGHTING_TILE_BYTES]) -> TileChromaSpan {
    let texels = MODEL_LIGHTING_TILE_BYTES / 4;
    let mut red_dominant = 0usize;
    let mut blue_dominant = 0usize;
    let mut min_r_minus_b = i16::MAX;
    let mut max_r_minus_b = i16::MIN;
    for i in 0..texels {
        let o = i * 4;
        let r = i16::from(tile[o]);
        let b = i16::from(tile[o + 2]);
        let d = r - b;
        min_r_minus_b = min_r_minus_b.min(d);
        max_r_minus_b = max_r_minus_b.max(d);
        if r > b {
            red_dominant += 1;
        } else if b > r {
            blue_dominant += 1;
        }
    }
    TileChromaSpan {
        texels,
        red_dominant,
        blue_dominant,
        min_r_minus_b,
        max_r_minus_b,
        alpha: tile[3],
    }
}

pub fn lit_fragment_white_from_tile_texel(
    tile: &[u8; MODEL_LIGHTING_TILE_BYTES],
    texel: usize,
) -> Option<[f32; 3]> {
    let o = texel.checked_mul(4)?;
    if o + 3 >= tile.len() {
        return None;
    }
    let sample = [
        f32::from(tile[o]) / 255.0,
        f32::from(tile[o + 1]) / 255.0,
        f32::from(tile[o + 2]) / 255.0,
    ];
    let light = decode_model_lighting_sample(sample);
    let albedo = lit_albedo([1.0, 1.0, 1.0], [1.0, 1.0, 1.0]);
    Some(lit_fragment_color(albedo, light, [0.0, 0.0, 0.0]))
}

pub fn lit_fragment_white_from_tile_normal(
    tile: &[u8; MODEL_LIGHTING_TILE_BYTES],
    normal: [f32; 3],
) -> Option<[f32; 3]> {
    let index = model_lighting_local_tile_nearest_index(normal)?;
    lit_fragment_white_from_tile_texel(tile, index)
}

pub fn lit_fragment_white_sun_add(
    tile: &[u8; MODEL_LIGHTING_TILE_BYTES],
    light_diffuse_rgb: [f32; 3],
    n_dot_l_sat: f32,
) -> [f32; 3] {
    let sample = mean_tile_rgba01(tile);
    let ambient = decode_model_lighting_sample(sample);
    let alpha = f32::from(tile[3]) / 255.0;
    let lighting = lit_sun_lighting(ambient, alpha, n_dot_l_sat, light_diffuse_rgb);
    let albedo = lit_albedo([1.0, 1.0, 1.0], [1.0, 1.0, 1.0]);
    lit_fragment_color(albedo, lighting, [0.0, 0.0, 0.0])
}

pub fn census_lit_fragment_tiles(
    samples: &[SmodelLightingSample],
) -> Option<LitFragmentTileCensus> {
    if samples.is_empty() {
        return None;
    }
    let mut lum_min = f32::INFINITY;
    let mut lum_max = 0.0f32;
    let mut lum_sum = 0.0f32;
    for sample in samples {
        let rgb = lit_fragment_mid_grey_from_tile(&sample.tile_rgba);
        let lum = rgb[0] + rgb[1] + rgb[2];
        lum_min = lum_min.min(lum);
        lum_max = lum_max.max(lum);
        lum_sum += lum;
    }
    let n = samples.len() as f32;
    Some(LitFragmentTileCensus {
        tiles: samples.len(),
        lum_min,
        lum_max,
        lum_mean: lum_sum / n,
    })
}

impl SmodelLightingCensus {
    fn absorb(&mut self, other: Self) {
        self.lit += other.lit;
        self.blocked_unmodelled_row += other.blocked_unmodelled_row;
        self.blocked_truncated += other.blocked_truncated;
        self.blocked_no_live_corner += other.blocked_no_live_corner;
        self.corners_sight_suppressed += other.corners_sight_suppressed;
        self.corners_needing_sight += other.corners_needing_sight;
    }

    fn record_block(&mut self, reason: BlockedReason) {
        match reason {
            BlockedReason::UnmodelledRowBranch => self.blocked_unmodelled_row += 1,
            BlockedReason::TruncatedZoneData => self.blocked_truncated += 1,
            BlockedReason::NoLiveCorner => self.blocked_no_live_corner += 1,
        }
    }
}

pub fn build_smodel_lighting_samples(
    grid: &GridView<'_>,
    origins: &[(usize, [f32; 3])],
) -> (Vec<SmodelLightingSample>, SmodelLightingCensus) {
    build_smodel_lighting_samples_with_sight(grid, origins, None)
}

pub fn build_smodel_lighting_samples_with_sight(
    grid: &GridView<'_>,
    origins: &[(usize, [f32; 3])],
    sight_clear: Option<&(dyn Fn([f32; 3], [f32; 3]) -> bool + Sync)>,
) -> (Vec<SmodelLightingSample>, SmodelLightingCensus) {
    let mut census = SmodelLightingCensus {
        candidates: origins.len(),
        ..SmodelLightingCensus::default()
    };
    if origins.is_empty() {
        return (Vec::new(), census);
    }

    let pool = ComputeTaskPool::get_or_init(TaskPool::default);
    let chunk_size = origins.len().div_ceil(pool.thread_num().max(1)).max(1);

    let per_chunk = pool.scope(|scope| {
        for chunk in origins.chunks(chunk_size) {
            scope.spawn(async move {
                let mut tiles = Vec::with_capacity(chunk.len());
                let mut census = SmodelLightingCensus::default();
                for &(authored_slot, lighting_origin) in chunk {
                    let sight_clear =
                        sight_clear.map(|clear| clear as &dyn Fn([f32; 3], [f32; 3]) -> bool);
                    match sample_light_grid_with_sight(grid, lighting_origin, sight_clear) {
                        Ok(sample) => {
                            census.lit += 1;
                            census.corners_needing_sight +=
                                usize::from(sample.corners_needing_sight);
                            census.corners_sight_suppressed +=
                                usize::from(sample.corners_sight_suppressed);
                            tiles.push(SmodelLightingSample {
                                authored_slot,
                                lighting_origin,
                                tile_rgba: sample.tile,
                                colors: sample.colors,
                                packed_lighting: [
                                    sample.compressed[0],
                                    sample.compressed[1],
                                    sample.compressed[2],
                                    0xff,
                                ],
                                path: sample.path,
                                lighting_sh: sample.lighting_sh,
                                vertex_lighting: None,
                            });
                        }
                        Err(reason) => census.record_block(reason),
                    }
                }
                (tiles, census)
            });
        }
    });
    let mut tiles = Vec::new();
    for (chunk_tiles, chunk_census) in per_chunk {
        tiles.extend(chunk_tiles);
        census.absorb(chunk_census);
    }
    (tiles, census)
}

pub fn packed_lighting_for_origins(
    grid: &GridView<'_>,
    origins: &[[f32; 3]],
    sight_clear: Option<&(dyn Fn([f32; 3], [f32; 3]) -> bool + Sync)>,
) -> (Vec<Option<[u8; 4]>>, SmodelLightingCensus) {
    let indexed: Vec<(usize, [f32; 3])> = origins.iter().copied().enumerate().collect();
    let (tiles, census) = build_smodel_lighting_samples_with_sight(grid, &indexed, sight_clear);
    let mut packed = vec![None; origins.len()];
    for tile in tiles {
        if let Some(slot) = packed.get_mut(tile.authored_slot) {
            *slot = Some(tile.packed_lighting);
        }
    }
    (packed, census)
}

pub fn collect_smodel_lighting_origins(
    stream: &ZoneStream<'_>,
    world: GfxWorldGeometry,
) -> Vec<(usize, [f32; 3])> {
    let Some(smodel_insts) = world.smodel_insts else {
        return Vec::new();
    };
    let mut out = Vec::with_capacity(world.smodel_count);
    for i in 0..world.smodel_count {
        let inst = smodel_insts.at(i * GFX_STATIC_MODEL_INST_SIZE);
        let Ok(bytes) = stream.slice_at(inst, 0, GFX_STATIC_MODEL_INST_SIZE) else {
            continue;
        };
        let Some(origin) = lighting_origin_from_inst_bytes(bytes) else {
            continue;
        };
        out.push((i, origin));
    }
    out
}

pub fn build_smodel_lighting_samples_from_world(
    stream: &ZoneStream<'_>,
    world: GfxWorldGeometry,
) -> Option<(Vec<SmodelLightingSample>, SmodelLightingCensus)> {
    let owned = OwnedLightGrid::from_stream(stream, world.light_grid)?;
    let origins = collect_smodel_lighting_origins(stream, world);
    if origins.is_empty() {
        return Some((
            Vec::new(),
            SmodelLightingCensus {
                candidates: 0,
                ..SmodelLightingCensus::default()
            },
        ));
    }
    Some(build_smodel_lighting_samples(&owned.view(), &origins))
}

pub fn collect_iw5_smodel_lighting_origins(
    stream: &fastfile_iw5::ZoneStream<'_>,
    world: fastfile_iw5::GfxWorldGeometry,
) -> Vec<(usize, [f32; 3])> {
    let Some(smodel_insts) = world.smodel_insts else {
        return Vec::new();
    };
    let stride = fastfile_iw5::size::GFX_STATIC_MODEL_INST;
    let mut out = Vec::with_capacity(world.smodel_count);
    for i in 0..world.smodel_count {
        let inst = smodel_insts.at(i * stride);
        let Ok(bytes) = stream.slice_at(inst, 0, stride) else {
            continue;
        };
        let Some(origin) = lighting_origin_from_inst_bytes(bytes) else {
            continue;
        };
        out.push((i, origin));
    }
    out
}

pub fn collect_t5_smodel_lighting_origins(
    stream: &fastfile_t5::ZoneStream<'_>,
    world: fastfile_t5::GfxWorldGeometry,
) -> Vec<(usize, [f32; 3])> {
    let Some(smodel_insts) = world.smodel_insts else {
        return Vec::new();
    };
    let stride = fastfile_t5::size::GFX_STATIC_MODEL_INST;
    let mut out = Vec::with_capacity(world.smodel_count);
    for i in 0..world.smodel_count {
        let inst = smodel_insts.at(i * stride);
        let Ok(bytes) = stream.slice_at(inst, 0, stride) else {
            continue;
        };
        let Some(origin) = lighting_origin_from_inst_bytes(bytes) else {
            continue;
        };
        out.push((i, origin));
    }
    out
}
