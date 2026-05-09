#import bevy_sprite::mesh2d_vertex_output::VertexOutput

@group(#{MATERIAL_BIND_GROUP}) @binding(0) var color_tex: texture_2d<f32>;
@group(#{MATERIAL_BIND_GROUP}) @binding(1) var color_sampler: sampler;
@group(#{MATERIAL_BIND_GROUP}) @binding(2) var normal_tex: texture_2d<f32>;
@group(#{MATERIAL_BIND_GROUP}) @binding(3) var normal_sampler: sampler;
@group(#{MATERIAL_BIND_GROUP}) @binding(4) var<uniform> params: AseLitParams;

struct AseLitParams {
    /// Atlas rect in pixel space: xy = min, zw = size.
    uv_rect: vec4<f32>,
    /// Per-axis flip: -1.0 mirrors that axis, 1.0 leaves it.
    flip: vec2<f32>,
    _pad: vec2<f32>,
    tint: vec4<f32>,
};

/// Sample atlas color for a given mesh local uv ([0,1]). Honours flip.
fn ase_sample_color(local_uv: vec2<f32>) -> vec4<f32> {
    let dims = vec2<f32>(textureDimensions(color_tex));
    let mirror = vec2<f32>(
        select(local_uv.x, 1.0 - local_uv.x, params.flip.x < 0.0),
        select(local_uv.y, 1.0 - local_uv.y, params.flip.y < 0.0),
    );
    let pixel = params.uv_rect.xy + mirror * params.uv_rect.zw;
    let uv = pixel / dims;
    return textureSample(color_tex, color_sampler, uv);
}

/// Sample atlas normal for a given mesh local uv ([0,1]) and decode +
/// flip-correct it. Returns tangent-space normal in [-1, 1].
fn ase_sample_normal(local_uv: vec2<f32>) -> vec3<f32> {
    let dims = vec2<f32>(textureDimensions(normal_tex));
    let mirror = vec2<f32>(
        select(local_uv.x, 1.0 - local_uv.x, params.flip.x < 0.0),
        select(local_uv.y, 1.0 - local_uv.y, params.flip.y < 0.0),
    );
    let pixel = params.uv_rect.xy + mirror * params.uv_rect.zw;
    let uv = pixel / dims;
    let raw = textureSample(normal_tex, normal_sampler, uv).xyz;
    var n = raw * 2.0 - 1.0;
    n.x *= params.flip.x;
    n.y *= params.flip.y;
    return n;
}

@fragment
fn fragment(v: VertexOutput) -> @location(0) vec4<f32> {
    let color = ase_sample_color(v.uv);
    // Sample the normal so the binding is referenced (silences unused warnings)
    // and so users hex-editing this shader to add lighting only need to
    // touch the return statement.
    let _n = ase_sample_normal(v.uv);
    return color * params.tint;
}
