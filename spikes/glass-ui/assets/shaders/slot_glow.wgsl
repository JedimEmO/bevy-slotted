// "Legendary" slot material: SDF rounded-rect border ring, animated shimmer sweep and a soft
// outer glow, all in one quad. The node this is attached to is inset by `-glow_px` around the
// slot so the outer glow has room to fall off.

#import bevy_render::view::View
#import bevy_render::globals::Globals
#import bevy_ui::ui_vertex_output::UiVertexOutput

@group(0) @binding(0) var<uniform> view: View;
@group(0) @binding(1) var<uniform> globals: Globals;

struct SlotGlowMaterial {
    rarity_color: vec4<f32>,
    // x: inset of the slot rect inside this quad, in px (the glow padding)
    // y: corner radius of the slot rect, in px
    // z: border ring thickness, in px
    // w: shimmer speed
    params: vec4<f32>,
};

@group(1) @binding(0) var<uniform> material: SlotGlowMaterial;

fn sd_rounded_box(p: vec2<f32>, b: vec2<f32>, r: f32) -> f32 {
    let q = abs(p) - b + vec2<f32>(r);
    return min(max(q.x, q.y), 0.0) + length(max(q, vec2<f32>(0.0))) - r;
}

@fragment
fn fragment(in: UiVertexOutput) -> @location(0) vec4<f32> {
    let inset = material.params.x;
    let radius = material.params.y;
    let thickness = material.params.z;
    let speed = material.params.w;

    let p = (in.uv - vec2<f32>(0.5)) * in.size;
    let half_size = in.size * 0.5 - vec2<f32>(inset);
    let d = sd_rounded_box(p, half_size, radius);

    let rarity = material.rarity_color.rgb;

    // Outer glow: exponential falloff, gated to *outside* the slot rect so the interior stays
    // readable (an ungated exp(-max(d,0)) is 1.0 everywhere inside and floods the slot).
    let outside = smoothstep(-1.0, 1.0, d);
    let glow = exp(-max(d, 0.0) / max(inset * 0.45, 1.0)) * outside;
    let glow_pulse = 0.75 + 0.25 * sin(globals.time * 1.7);
    var rgb = rarity * glow * 0.7 * glow_pulse;
    var alpha = clamp(glow * 0.75 * material.rarity_color.a, 0.0, 1.0);

    // Border ring: a band of `thickness` px just inside the edge.
    let ring = 1.0 - smoothstep(0.0, 1.2, abs(d + thickness * 0.5) - thickness * 0.5);

    // Shimmer: a diagonal band sweeping across the ring.
    let sweep = fract((in.uv.x + in.uv.y) * 0.5 - globals.time * speed);
    let shimmer = pow(clamp(1.0 - abs(sweep - 0.5) * 4.0, 0.0, 1.0), 3.0);

    let ring_color = mix(rarity, vec3<f32>(1.0), 0.15 + 0.85 * shimmer);
    rgb = mix(rgb, ring_color * (1.0 + shimmer * 1.6), ring);
    alpha = max(alpha, ring);

    // Faint interior wash so the slot reads as "special" even away from the ring.
    let inside = 1.0 - smoothstep(-thickness * 2.0, 0.0, d);
    rgb += rarity * inside * 0.05;
    alpha = max(alpha, inside * 0.07);

    if alpha <= 0.002 {
        discard;
    }
    return vec4<f32>(rgb, alpha);
}
