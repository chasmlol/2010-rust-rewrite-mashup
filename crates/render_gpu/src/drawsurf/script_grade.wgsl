struct Grade {
    script: vec4<f32>,
    t6_film: vec4<f32>,
    controls: array<vec4<f32>, 14>,
}
@group(0) @binding(0) var scene: texture_2d<f32>;
@group(0) @binding(1) var scene_sampler: sampler;
@group(0) @binding(2) var<uniform> grade: Grade;
struct Vertex { @builtin(position) position: vec4<f32>, @location(0) uv: vec2<f32> }
@vertex fn vertex(@builtin(vertex_index) i: u32) -> Vertex {
    let uv = vec2<f32>(f32((i << 1u) & 2u), f32(i & 2u));
    return Vertex(vec4<f32>(uv * vec2<f32>(2.0, -2.0) + vec2<f32>(-1.0, 1.0), 0.0, 1.0), uv);
}
fn script_grade(sampled: vec3<f32>) -> vec3<f32> {
    let g = grade.script;
    let y = dot(sampled, vec3<f32>(0.299, 0.587, 0.114));
    let i = dot(sampled, vec3<f32>(0.596, -0.274, -0.322));
    let q = dot(sampled, vec3<f32>(0.211, -0.523, 0.312));
    let ri = i * cos(g.x) - q * sin(g.x);
    let rq = i * sin(g.x) + q * cos(g.x);
    let rotated = vec3<f32>(y + 0.956 * ri + 0.621 * rq, y - 0.272 * ri - 0.647 * rq, y - 1.106 * ri + 1.703 * rq);
    let original = vec3<f32>(y + 0.956 * i + 0.621 * q, y - 0.272 * i - 0.647 * q, y - 1.106 * i + 1.703 * q);
    let hue = sampled + rotated - original;
    let saturated = mix(vec3<f32>(y), hue, g.w);
    return pow(max(saturated * exp2(g.z), vec3<f32>(0.0)), vec3<f32>(1.0 / g.y));
}
fn t6_film(encoded: vec3<f32>) -> vec3<f32> {
    let c = grade.controls;
    let luma = dot(encoded, vec3<f32>(0.21258545, 0.7151947, 0.07221985));
    let ramp = saturate(vec4<f32>(luma) * c[0].xzyw + c[1].xzyw);
    var w = vec3<f32>(ramp.x, ramp.y * ramp.w, ramp.z);
    w = w * w * (3.0 - 2.0 * w);
    w = w / (w.x + w.y + w.z);
    let linear = vec4<f32>(encoded * encoded, 1.0);
    let r = saturate(dot(vec3<f32>(dot(linear, c[2]), dot(linear, c[3]), dot(linear, c[4])), w));
    let g = saturate(dot(vec3<f32>(dot(linear, c[5]), dot(linear, c[6]), dot(linear, c[7])), w));
    let b = saturate(dot(vec3<f32>(dot(linear, c[8]), dot(linear, c[9]), dot(linear, c[10])), w));
    let curved = exp2(log2(vec3<f32>(r, g, b)) * c[11].xyz);
    let y = dot(curved, c[12].xyz);
    let saturated = mix(vec3<f32>(y), curved, c[12].w);
    return sqrt(saturate(mix(linear.rgb, saturated, c[13].xyz)));
}
@fragment fn fragment(v: Vertex) -> @location(0) vec4<f32> {
    let sampled = textureSample(scene, scene_sampler, v.uv);
    var rgb = script_grade(sampled.rgb);
    if grade.t6_film.x != 0.0 {
        rgb = t6_film(saturate(rgb));
    }
    return vec4<f32>(rgb, sampled.a);
}
