// Chamfered-corner panel material for bevy_ui 0.19 (slotted's neon theme).
//
// A box with diagonal cuts on the chosen corners, an inner border and an
// optional accent bar along the bottom edge with a glow that bleeds up into
// the fill. Everything is one signed-distance function evaluated in node
// pixels, so the bar is clipped by the same chamfer as the fill.

#import bevy_ui::ui_vertex_output::UiVertexOutput

struct CutCornerMaterial {
    fill: vec4<f32>,
    border: vec4<f32>,
    bar: vec4<f32>,
    // x border width, y cut length, z bar height, w glow; all in px.
    params: vec4<f32>,
    // Cut flags: top-left, top-right, bottom-right, bottom-left.
    corners: vec4<f32>,
};

@group(1) @binding(0) var<uniform> material: CutCornerMaterial;

// Signed distance to a box of half-size `b` centred on the origin whose
// flagged corners are chamfered `cut` px along each edge. `p.y` grows
// downward, as `UiVertexOutput.uv` does.
fn sd_cut_box(p: vec2<f32>, b: vec2<f32>, cut: f32, corners: vec4<f32>) -> f32 {
    let q = abs(p) - b;
    var d = max(q.x, q.y);
    let reach = b.x + b.y - cut;
    let inv_sqrt2 = 0.70710678;
    if corners.x > 0.5 {
        d = max(d, (-p.x - p.y - reach) * inv_sqrt2);
    }
    if corners.y > 0.5 {
        d = max(d, (p.x - p.y - reach) * inv_sqrt2);
    }
    if corners.z > 0.5 {
        d = max(d, (p.x + p.y - reach) * inv_sqrt2);
    }
    if corners.w > 0.5 {
        d = max(d, (-p.x + p.y - reach) * inv_sqrt2);
    }
    return d;
}

@fragment
fn fragment(in: UiVertexOutput) -> @location(0) vec4<f32> {
    let half_size = in.size * 0.5;
    let p = (in.uv - vec2<f32>(0.5)) * in.size;
    let border_width = material.params.x;
    let cut = material.params.y;
    let bar_height = material.params.z;
    let glow = material.params.w;

    let d = sd_cut_box(p, half_size, cut, material.corners);
    let coverage = 1.0 - smoothstep(-0.5, 0.5, d);
    if coverage <= 0.0 {
        discard;
    }

    var color = material.fill;

    // Distance up from the bottom edge, in px.
    let from_bottom = half_size.y - p.y;
    if glow > 0.0 && from_bottom > bar_height && from_bottom < bar_height + glow {
        let t = 1.0 - (from_bottom - bar_height) / glow;
        let haze = material.bar.a * 0.6 * t * t;
        color = vec4<f32>(mix(color.rgb, material.bar.rgb, haze), max(color.a, haze));
    }
    if from_bottom <= bar_height {
        let bar_cov = material.bar.a * (1.0 - smoothstep(bar_height - 0.5, bar_height + 0.5, from_bottom));
        color = vec4<f32>(mix(color.rgb, material.bar.rgb, bar_cov), max(color.a, bar_cov));
    }

    // Inner border: the band between the edge and `border_width` in.
    if border_width > 0.0 && material.border.a > 0.0 {
        let inner = smoothstep(-border_width - 0.5, -border_width + 0.5, d);
        let edge = inner * material.border.a;
        color = vec4<f32>(mix(color.rgb, material.border.rgb, edge), max(color.a, edge));
    }

    return vec4<f32>(color.rgb, color.a * coverage);
}
