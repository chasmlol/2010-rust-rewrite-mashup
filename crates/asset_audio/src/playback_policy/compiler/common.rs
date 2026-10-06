use super::*;

pub(super) fn native_curve(
    row: &CapturedAlias,
) -> Result<SpatialPlaybackPolicy, SpatialPolicyFailure> {
    let curve = row
        .volume_falloff
        .as_ref()
        .filter(|curve| !curve.knots.is_empty())
        .ok_or(SpatialPolicyFailure::MissingFalloffCurve)?;
    if !row.dist_min.is_finite()
        || !row.dist_max.is_finite()
        || curve
            .knots
            .iter()
            .chain(row.near_falloff.iter().flat_map(|near| near.knots.iter()))
            .any(|(x, y)| !x.is_finite() || !y.is_finite())
    {
        return Err(SpatialPolicyFailure::InvalidFalloffCurve);
    }
    let pack = |knots: &[(f32, f32)]| -> Arc<[[f32; 2]]> {
        knots
            .iter()
            .take(asset_iw4::SND_CURVE_MAX_KNOTS)
            .map(|&(x, y)| [x, y])
            .collect::<Vec<_>>()
            .into()
    };
    Ok(SpatialPlaybackPolicy {
        dist_min: row.dist_min,
        dist_max: row.dist_max,
        knots: pack(&curve.knots),
        near_knots: row.near_falloff.as_ref().map(|curve| pack(&curve.knots)),
    })
}

pub(super) fn iw_speaker_gains(row: &CapturedAlias) -> Option<[[f32; 2]; 2]> {
    row.stereo_speaker_gains.filter(|gains| {
        gains
            .iter()
            .flatten()
            .all(|gain| gain.is_finite() && *gain >= 0.0)
    })
}
