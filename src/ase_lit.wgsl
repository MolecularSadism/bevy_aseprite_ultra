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
    /// xy = directional light *travel* direction (unit) in tangent space; the
    /// shader uses -sun_dir.xy as the to-source direction. zw unused.
    sun_dir: vec4<f32>,
    /// rgb = directional source colour pre-multiplied by intensity.
    sun_color: vec4<f32>,
    /// rgb = ambient term added to the directional contribution.
    ambient: vec4<f32>,
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
    let albedo = ase_sample_color(v.uv) * params.tint;
    let n = normalize(ase_sample_normal(v.uv));
    // 2D Lambert against the screen-plane projection of the tangent-space
    // normal. `sun_dir.xy` is the source's travel direction, so the to-source
    // direction is `-sun_dir.xy`. Length zero ⇒ no directional contribution.
    let to_source = -params.sun_dir.xy;
    let dir_len = length(to_source);
    let lambert = select(0.0, max(0.0, dot(n.xy, to_source / max(dir_len, 1e-6))), dir_len > 1e-6);
    let lit = params.ambient.rgb + params.sun_color.rgb * lambert;
    return vec4<f32>(albedo.rgb * lit, albedo.a);
}
