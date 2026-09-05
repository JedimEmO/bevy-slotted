// Backdrop-blur glass panel material for bevy_ui 0.19.
//
// Key trick: the UI material fragment shader receives `@builtin(position)` in
// `UiVertexOutput.position`, which is the framebuffer pixel coordinate. Together with
// `view.viewport` from the group(0) view uniform that gives the node's screen-space UV for
// free, so we never have to push the node's screen rect into the material ourselves.

#import bevy_render::view::View
#import bevy_render::globals::Globals
#import bevy_ui::ui_vertex_output::UiVertexOutput

@group(0) @binding(0) var<uniform> view: View;
@group(0) @binding(1) var<uniform> globals: Globals;

struct GlassMaterial {
    tint: vec4<f32>,
    edge_color: vec4<f32>,
    // Blur radius expressed in texels of the (quarter-resolution) backdrop image.
    blur_radius: f32,
    // 0.0 disables the backdrop sample entirely (used for the no-blur measurement).
    blur_enabled: f32,
    _pad: vec2<f32>,
};

@group(1) @binding(0) var<uniform> material: GlassMaterial;
@group(1) @binding(1) var backdrop_texture: texture_2d<f32>;
@group(1) @binding(2) var backdrop_sampler: sampler;

// Signed distance to a rounded box centred on the origin.
// `r` follows UiVertexOutput.border_radius order: top-left, top-right, bottom-right, bottom-left.
fn sd_rounded_box(p: vec2<f32>, b: vec2<f32>, r: vec4<f32>) -> f32 {
    let top = select(r.x, r.y, p.x > 0.0);
    let bottom = select(r.w, r.z, p.x > 0.0);
    let radius = select(top, bottom, p.y > 0.0);
    let q = abs(p) - b + vec2<f32>(radius);
    return min(max(q.x, q.y), 0.0) + length(max(q, vec2<f32>(0.0))) - radius;
}

// 13-tap tent blur on an already-quarter-resolution source. This is the cheap stand-in for a
// real dual-Kawase chain: two rings of 4 taps plus the centre, weighted 4/2/1.
fn blur13(uv: vec2<f32>, texel: vec2<f32>, radius: f32) -> vec3<f32> {
    let o1 = texel * radius;
    let o2 = texel * radius * 2.0;
    var acc = textureSample(backdrop_texture, backdrop_sampler, uv).rgb * 4.0;

    acc += textureSample(backdrop_texture, backdrop_sampler, uv + vec2<f32>( o1.x,  o1.y)).rgb * 2.0;
    acc += textureSample(backdrop_texture, backdrop_sampler, uv + vec2<f32>(-o1.x,  o1.y)).rgb * 2.0;
    acc += textureSample(backdrop_texture, backdrop_sampler, uv + vec2<f32>( o1.x, -o1.y)).rgb * 2.0;
    acc += textureSample(backdrop_texture, backdrop_sampler, uv + vec2<f32>(-o1.x, -o1.y)).rgb * 2.0;

    acc += textureSample(backdrop_texture, backdrop_sampler, uv + vec2<f32>( o2.x,  0.0)).rgb;
    acc += textureSample(backdrop_texture, backdrop_sampler, uv + vec2<f32>(-o2.x,  0.0)).rgb;
    acc += textureSample(backdrop_texture, backdrop_sampler, uv + vec2<f32>( 0.0,  o2.y)).rgb;
    acc += textureSample(backdrop_texture, backdrop_sampler, uv + vec2<f32>( 0.0, -o2.y)).rgb;

    acc += textureSample(backdrop_texture, backdrop_sampler, uv + vec2<f32>( o2.x,  o2.y)).rgb;
    acc += textureSample(backdrop_texture, backdrop_sampler, uv + vec2<f32>(-o2.x,  o2.y)).rgb;
    acc += textureSample(backdrop_texture, backdrop_sampler, uv + vec2<f32>( o2.x, -o2.y)).rgb;
    acc += textureSample(backdrop_texture, backdrop_sampler, uv + vec2<f32>(-o2.x, -o2.y)).rgb;

    return acc / 24.0;
}

@fragment
fn fragment(in: UiVertexOutput) -> @location(0) vec4<f32> {
    let half_size = in.size * 0.5;
    let p = (in.uv - vec2<f32>(0.5)) * in.size;
    let d = sd_rounded_box(p, half_size, in.border_radius);

    // Antialiased coverage of the rounded rect.
    let coverage = 1.0 - smoothstep(-1.0, 1.0, d);
    if coverage <= 0.0 {
        discard;
    }

    // Screen-space UV of this fragment, derived from the framebuffer position.
    let screen_uv = (in.position.xy - view.viewport.xy) / view.viewport.zw;

    var backdrop = vec3<f32>(0.0);
    if material.blur_enabled > 0.5 {
        let texel = 1.0 / vec2<f32>(textureDimensions(backdrop_texture));
        backdrop = blur13(screen_uv, texel, material.blur_radius);
    }

    // Composite the tint over the (blurred) backdrop.
    var rgb = mix(backdrop, material.tint.rgb, material.tint.a);

    // 1px inner top highlight running around the top of the panel, fading toward the sides.
    let inner = 1.0 - smoothstep(0.0, 1.5, abs(d + 1.0));
    let top_bias = clamp(1.0 - in.uv.y * 2.4, 0.0, 1.0);
    rgb += material.edge_color.rgb * inner * material.edge_color.a * (0.25 + 0.75 * top_bias);

    // A very subtle vertical sheen so the glass is not perfectly flat.
    rgb += vec3<f32>(0.02, 0.03, 0.04) * (1.0 - in.uv.y) * 0.6;

    return vec4<f32>(rgb, coverage);
}
