struct Params { alpha: vec4<f32> }
@group(0) @binding(0) var history: texture_2d<f32>;
@group(0) @binding(1) var history_sampler: sampler;
@group(0) @binding(2) var<uniform> params: Params;
struct VertexOut {
    @builtin(position) position: vec4<f32>,
    @location(0) uv: vec2<f32>,
}
@vertex
fn vertex(@builtin(vertex_index) index: u32) -> VertexOut {
    let uv = vec2<f32>(f32((index << 1u) & 2u), f32(index & 2u));
    var out: VertexOut;
    out.position = vec4<f32>(uv * vec2<f32>(2.0, -2.0) + vec2<f32>(-1.0, 1.0), 0.0, 1.0);
    out.uv = uv;
    return out;
}
@fragment
fn fragment(in: VertexOut) -> @location(0) vec4<f32> {
    let saved = textureSample(history, history_sampler, in.uv).rgb;
    let grey = dot(saved, vec3<f32>(0.299, 0.587, 0.114));
    return vec4<f32>(saved + (vec3<f32>(grey) - saved) * 0.25, params.alpha.x);
}
