use super::frame_products::{FrameAssemblyInputs, MaterialFrameInputs};
use render_frame::{MaterialExecFrame, OutdoorLookup};

pub fn refresh(
    frame: &mut MaterialExecFrame,
    res: &MaterialFrameInputs,
    inputs: Option<&FrameAssemblyInputs>,
) {
    frame.code_sources.clone_from(&res.code_sources);
    frame.view_origin = res.view_origin;
    frame.float_time = res.float_time;
    frame.clip_from_world = res.clip_from_world;
    frame.view_from_world = res.view_from_world;
    frame.clip_from_view = res.clip_from_view;
    frame.outdoor = res.outdoor.map(|outdoor| OutdoorLookup {
        image: outdoor.image,
        lookup: outdoor.lookup,
    });
    frame.viewmodel_clip_from_world = res.viewmodel_clip_from_world;
    frame.viewmodel_near = res.viewmodel_near;
    frame.primary_lights.clear();
    frame.attenuation.clear();
    frame.local_light_bindings.clear();
    frame.reflection_probe_sh.clear();
    frame.spot_receivers.clear();
    let Some(inputs) = inputs else {
        frame.inv_image_height = None;
        frame.smodel_lighting_sh = Default::default();
        return;
    };
    frame.smodel_lighting_sh = inputs.smodel_lighting_sh.clone();
    frame.inv_image_height = inputs.inv_image_height;
    frame
        .primary_lights
        .extend_from_slice(&inputs.primary_lights);
    frame.attenuation.extend_from_slice(&inputs.attenuation);
    frame
        .local_light_bindings
        .clone_from(&inputs.local_light_bindings);
    frame
        .reflection_probe_sh
        .extend_from_slice(&inputs.reflection_probe_sh);
    frame.spot_receivers.clone_from(&res.spot_receivers);
}
