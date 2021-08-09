// Vertex shader

struct VertexInput {
    [[location(0)]] position: vec3<f32>;
    [[location(1)]] tex_coords: vec2<f32>;
};

struct VertexOutput {
    [[builtin(position)]] clip_position: vec4<f32>;
    [[location(0)]] tex_coords: vec2<f32>;
};

[[stage(vertex)]]
fn main(
    model: VertexInput,
) -> VertexOutput {
    var out: VertexOutput;
    out.tex_coords = model.tex_coords;
    out.clip_position = vec4<f32>(model.position, 1.0);
    return out;
}

// Fragment shader

type v2f = vec2<f32>;
type v4f = vec4<f32>;

[[block]]
struct Uniforms {
    blend: f32;
    flip: f32;
    mode: i32;
    //resized_window_scale: vec2<f32>;
    resized_window_scale_x: f32;
    resized_window_scale_y: f32;
    //bg: vec4<f32>;
    bg_r: f32;
    bg_g: f32;
    bg_b: f32;
    bg_a: f32;
};

[[group(0), binding(0)]]
var t_diffuse_a: texture_2d<f32>;
[[group(0), binding(1)]]
var t_diffuse_b: texture_2d<f32>;
[[group(0), binding(2)]]
var s_diffuse: sampler;
[[group(1), binding(0)]]
var<uniform> uniforms: Uniforms;

// saturate
fn clamp01(v: f32) -> f32 {
    return clamp(v, 0.0, 1.0);
}
 
fn rand(co: v2f) -> f32 {
    return fract(sin(dot(co ,v2f(12.9898, 78.233))) * 43758.5453);
}

// transitions

fn ts_crossfading(a: v4f, b: v4f, t: f32) -> v4f {
    return mix(a, b, v4f(t));
}

fn ts_smooth_crossfading(a: v4f, b: v4f, t: f32) -> v4f {
    return mix(a, b, v4f(smoothStep(0.0, 1.0, t)));
}

fn ts_roll(a: v4f, b: v4f, t: f32, pos: f32) -> v4f {
    return mix(a, b, v4f(step(1.0 - pos, t)));
}

fn ts_roll45(a: v4f, b: v4f, t: f32, posx: f32, posy: f32) -> v4f {
    let x = step(1.0 - posx, t);
    let y = step(1.0 - posy, t);
    return mix(a, b, v4f(clamp01(x + y)));
}

fn ts_sliding_door_out(a: v4f, b: v4f, t: f32, pos: f32) -> v4f {
    let t = t * 0.5;
    let forward = step(1.0 - pos, t);
    let back = step(pos, t);
    return mix(a, b, v4f(clamp01(forward + back)));
}

fn ts_sliding_door_in(a: v4f, b: v4f, t: f32, pos: f32) -> v4f {
    let t = (t * 0.5) + 0.5;
    let forward = step(pos, t);
    let back = step(1.0 - pos, t);
    return mix(a, b, v4f(clamp01(forward * back)));
}

fn ts_blind(a: v4f, b: v4f, t: f32, pos: f32) -> v4f {
    return mix(a, b, v4f(step((1.0 - pos) % 0.1 / 0.1, t)));
}

fn ts_box_out(a: v4f, b: v4f, t: f32, uv: v2f) -> v4f {
    let t = t * 0.5;
    let forward = step(1.0 - uv.x, t) + step(1.0 - uv.y, t);
    let back = step(uv.x, t) + step(uv.y, t);
    return mix(a, b, v4f(clamp01(forward + back)));
}

fn ts_box_in(a: v4f, b: v4f, t: f32, uv: v2f) -> v4f {
    let t = (t * 0.5) + 0.5;
    let forward = step(uv.x, t) * step(uv.y, t);
    let back = step(1.0 - uv.x, t) * step(1.0 - uv.y, t);
    return mix(a, b, v4f(clamp01(forward * back)));
}

// randomsquares
// https://gl-transitions.com/editor/randomsquares
// Author: gre
// License: MIT
fn ts_randomsquares(a: v4f, b: v4f, t: f32, uv: v2f) -> v4f {
    let size = v2f(8.0, 8.0);
    let smoothness = 0.5;
    let r = rand(floor(size * uv));
    let t = smoothStep(0.0, -smoothness, r - (t * (1.0 + smoothness)));
    return mix(a, b, v4f(clamp01(t)));
}

// angular
// https://gl-transitions.com/editor/angular
// Author: gre
// License: MIT
fn ts_angular(a: v4f, b: v4f, t: f32, uv: v2f) -> v4f {
    let PI = 3.141592653589;
    let offset = 90.0 * PI / 180.0;
    let angle = atan2(uv.y - 0.5, uv.x - 0.5) + offset;
    let normalizedAngle = (angle + PI) / (2.0 * PI);
    let normalizedAngle = normalizedAngle - floor(normalizedAngle);
    return mix(a, b, v4f(step(normalizedAngle, t)));
}

[[stage(fragment)]]
fn main(in: VertexOutput) -> [[location(0)]] v4f {
    let uv = v2f(
        (0.5 + ((in.tex_coords.x - 0.5) * uniforms.resized_window_scale_x)),
        (0.5 + ((in.tex_coords.y - 0.5) * uniforms.resized_window_scale_y))
    );

    let out_of_bounds = v4f(clamp01(
        step(uv.x, 0.0) + step(1.0 - uv.x, 0.0) +
        step(uv.y, 0.0) + step(1.0 - uv.y, 0.0)
    ));
    let bg = v4f(uniforms.bg_r, uniforms.bg_g, uniforms.bg_b, uniforms.bg_a);
    let src_a = mix(textureSample(t_diffuse_a, s_diffuse, uv), bg, out_of_bounds);
    let src_b = mix(textureSample(t_diffuse_b, s_diffuse, uv), bg, out_of_bounds);

    let a = mix(src_a, src_b, v4f(uniforms.flip));
    let b = mix(src_b, src_a, v4f(uniforms.flip));
    let t = mix(uniforms.blend, (1.0 - uniforms.blend), uniforms.flip);

    var ret: v4f;
    switch (uniforms.mode) {
        case 0: { ret = ts_crossfading(a, b, t); }
        case 1: { ret = ts_smooth_crossfading(a, b, t); }
        case 2: { ret = ts_roll(a, b, t, uv.x); }
        case 3: { ret = ts_roll(a, b, t, uv.y); }
        case 4: { ret = ts_roll(a, b, t, 1.0 - uv.x); }
        case 5: { ret = ts_roll(a, b, t, 1.0 - uv.y); }
        case 6: { ret = ts_roll45(a, b, t, uv.x, uv.y); }
        case 7: { ret = ts_roll45(a, b, t, uv.x, 1.0 - uv.y); }
        case 8: { ret = ts_roll45(a, b, t, 1.0 - uv.x, uv.y); }
        case 9: { ret = ts_roll45(a, b, t, 1.0 - uv.x, 1.0 - uv.y); }
        case 10: { ret = ts_sliding_door_out(a, b, t, uv.x); }
        case 11: { ret = ts_sliding_door_out(a, b, t, uv.y); }
        case 12: { ret = ts_sliding_door_in(a, b, t, uv.x); }
        case 13: { ret = ts_sliding_door_in(a, b, t, uv.y); }
        case 14: { ret = ts_blind(a, b, t, uv.x); }
        case 15: { ret = ts_blind(a, b, t, uv.y); }
        case 16: { ret = ts_blind(a, b, t, 1.0 - uv.x); }
        case 17: { ret = ts_blind(a, b, t, 1.0 - uv.y); }
        case 18: { ret = ts_box_out(a, b, t, uv); }
        case 19: { ret = ts_box_in(a, b, t, uv); }
        case 20: { ret = ts_randomsquares(a, b, t, uv); }
        case 21: { ret = ts_angular(a, b, t, uv); }
        default: { ret = ts_crossfading(a, b, t); }
    }
    return ret;
}
