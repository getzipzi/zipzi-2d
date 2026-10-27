struct Uniform {
    view_a: vec4<f32>,
    view_b: vec4<f32>,
    mask_a: vec4<f32>,
    mask_b: vec4<f32>,
    mask_region: vec4<f32>,
    base_color: vec4<f32>,
    multiply_color: vec4<f32>,
    screen_color: vec4<f32>,
    opacity: vec4<f32>,
    flags: vec4<u32>,
    blend_mode: vec4<u32>,
    alpha_mode: vec4<u32>,
};

@group(0) @binding(0) var<uniform> u: Uniform;
@group(0) @binding(1) var source_tex: texture_2d<f32>;
@group(0) @binding(2) var source_sampler: sampler;
@group(0) @binding(3) var mask_tex: texture_2d<f32>;
@group(0) @binding(4) var mask_sampler: sampler;
@group(0) @binding(5) var destination_tex: texture_2d<f32>;

struct VertexOut {
    @builtin(position) position: vec4<f32>,
    @location(0) uv: vec2<f32>,
    @location(1) mask_uv: vec2<f32>,
};

@vertex
fn vs_draw(
    @location(0) pos: vec2<f32>,
    @location(1) uv: vec2<f32>,
) -> VertexOut {
    var out: VertexOut;
    let clip = vec2<f32>(
        dot(u.view_a.xy, pos) + u.view_a.w,
        dot(u.view_b.xy, pos) + u.view_b.w,
    );
    out.position = vec4<f32>(clip, 0.0, 1.0);
    out.uv = vec2<f32>(uv.x, 1.0 - uv.y);
    out.mask_uv = vec2<f32>(
        dot(u.mask_a.xy, pos) + u.mask_a.w,
        dot(u.mask_b.xy, pos) + u.mask_b.w,
    );
    return out;
}

fn mask_alpha(in: VertexOut) -> f32 {
    if u.flags.x == 0u {
        return 1.0;
    }
    if any(in.mask_uv < vec2<f32>(0.0)) || any(in.mask_uv > vec2<f32>(1.0)) {
        return select(0.0, 1.0, u.flags.z != 0u);
    }
    let texture_size = vec2<f32>(textureDimensions(mask_tex));
    let half_texel = 0.5 / max(texture_size * u.mask_region.zw, vec2<f32>(1.0));
    let uv = clamp(in.mask_uv, half_texel, vec2<f32>(1.0) - half_texel);
    let alpha = textureSampleLevel(mask_tex, mask_sampler, u.mask_region.xy + uv * u.mask_region.zw, 0.0).a;
    return select(alpha, 1.0 - alpha, u.flags.z != 0u);
}

fn to_straight(value: vec4<f32>) -> vec4<f32> {
    if abs(value.a) < 0.00001 {
        return vec4<f32>(0.0);
    }
    return vec4<f32>(value.rgb / value.a, value.a);
}

fn color_burn(source: f32, destination: f32) -> f32 {
    if abs(destination - 1.0) < 0.000001 {
        return 1.0;
    }
    if abs(source) < 0.000001 {
        return 0.0;
    }
    return 1.0 - min(1.0, (1.0 - destination) / source);
}

fn color_dodge(source: f32, destination: f32) -> f32 {
    if destination <= 0.0 {
        return 0.0;
    }
    if source >= 1.0 {
        return 1.0;
    }
    return min(1.0, destination / (1.0 - source));
}

fn soft_light(source: f32, destination: f32) -> f32 {
    let low = destination - (1.0 - 2.0 * source) * destination * (1.0 - destination);
    let mid = destination + (2.0 * source - 1.0) * destination * ((16.0 * destination - 12.0) * destination + 3.0);
    let high = destination + (2.0 * source - 1.0) * (sqrt(max(destination, 0.0)) - destination);
    return select(select(high, mid, destination <= 0.25), low, source <= 0.5);
}

fn luma(value: vec3<f32>) -> f32 {
    return dot(value, vec3<f32>(0.30, 0.59, 0.11));
}

fn saturation(value: vec3<f32>) -> f32 {
    return max(value.r, max(value.g, value.b)) - min(value.r, min(value.g, value.b));
}

fn clip_color(input_value: vec3<f32>) -> vec3<f32> {
    var value = input_value;
    let lum = luma(value);
    let hi = max(value.r, max(value.g, value.b));
    let lo = min(value.r, min(value.g, value.b));
    if lo < 0.0 && lum != lo {
        value = vec3<f32>(lum) + (value - vec3<f32>(lum)) * lum / (lum - lo);
    }
    if hi > 1.0 && hi != lum {
        value = vec3<f32>(lum) + (value - vec3<f32>(lum)) * (1.0 - lum) / (hi - lum);
    }
    return value;
}

fn set_luma(value: vec3<f32>, lum: f32) -> vec3<f32> {
    return clip_color(value + vec3<f32>(lum - luma(value)));
}

fn set_saturation(value: vec3<f32>, sat: f32) -> vec3<f32> {
    let hi = max(value.r, max(value.g, value.b));
    let lo = min(value.r, min(value.g, value.b));
    let mid = value.r + value.g + value.b - hi - lo;
    let out_hi = select(0.0, sat, lo < hi);
    let out_mid = select(0.0, (mid - lo) * sat / max(hi - lo, 0.000001), lo < hi);
    if value.r == hi {
        return select(vec3<f32>(out_hi, 0.0, out_mid), vec3<f32>(out_hi, out_mid, 0.0), value.b < value.g);
    }
    if value.g == hi {
        return select(vec3<f32>(out_mid, out_hi, 0.0), vec3<f32>(0.0, out_hi, out_mid), value.r < value.b);
    }
    return select(vec3<f32>(0.0, out_mid, out_hi), vec3<f32>(out_mid, 0.0, out_hi), value.g < value.r);
}

fn color_blend(source: vec3<f32>, destination: vec3<f32>, mode: u32) -> vec3<f32> {
    if mode == 0u { return source; }
    if mode == 1u || mode == 3u { return min(source + destination, vec3<f32>(1.0)); }
    if mode == 2u || mode == 6u { return source * destination; }
    if mode == 4u { return source + destination; }
    if mode == 5u { return min(source, destination); }
    if mode == 7u { return vec3<f32>(color_burn(source.r, destination.r), color_burn(source.g, destination.g), color_burn(source.b, destination.b)); }
    if mode == 8u { return max(vec3<f32>(0.0), source + destination - vec3<f32>(1.0)); }
    if mode == 9u { return max(source, destination); }
    if mode == 10u { return source + destination - source * destination; }
    if mode == 11u { return vec3<f32>(color_dodge(source.r, destination.r), color_dodge(source.g, destination.g), color_dodge(source.b, destination.b)); }
    if mode == 12u { return mix(2.0 * source * destination, vec3<f32>(1.0) - 2.0 * (vec3<f32>(1.0) - source) * (vec3<f32>(1.0) - destination), step(vec3<f32>(0.5), destination)); }
    if mode == 13u { return vec3<f32>(soft_light(source.r, destination.r), soft_light(source.g, destination.g), soft_light(source.b, destination.b)); }
    if mode == 14u { return mix(2.0 * source * destination, vec3<f32>(1.0) - 2.0 * (vec3<f32>(1.0) - source) * (vec3<f32>(1.0) - destination), step(vec3<f32>(0.5), source)); }
    if mode == 15u { return mix(max(vec3<f32>(0.0), 2.0 * source + destination - vec3<f32>(1.0)), min(vec3<f32>(1.0), 2.0 * (source - vec3<f32>(0.5)) + destination), step(vec3<f32>(0.5), source)); }
    if mode == 16u { return set_luma(set_saturation(source, saturation(destination)), luma(destination)); }
    return set_luma(source, luma(destination));
}

fn alpha_blend(color: vec3<f32>, source: vec4<f32>, destination: vec4<f32>, mode: u32) -> vec4<f32> {
    var weights: vec3<f32>;
    if mode == 1u {
        weights = vec3<f32>(source.a * destination.a, 0.0, destination.a * (1.0 - source.a));
    } else if mode == 2u {
        weights = vec3<f32>(0.0, 0.0, destination.a * (1.0 - source.a));
    } else if mode == 3u {
        weights = vec3<f32>(min(source.a, destination.a), max(source.a - destination.a, 0.0), max(destination.a - source.a, 0.0));
    } else if mode == 4u {
        weights = vec3<f32>(max(source.a + destination.a - 1.0, 0.0), min(source.a, 1.0 - destination.a), min(destination.a, 1.0 - source.a));
    } else {
        weights = vec3<f32>(source.a * destination.a, source.a * (1.0 - destination.a), destination.a * (1.0 - source.a));
    }
    return vec4<f32>(
        color * weights.x + source.rgb * weights.y + destination.rgb * weights.z,
        weights.x + weights.y + weights.z,
    );
}

fn composite_blend(source: vec4<f32>, destination: vec4<f32>, color_mode: u32, alpha_mode: u32) -> vec4<f32> {
    if color_mode == 1u {
        return vec4<f32>(source.rgb * source.a + destination.rgb * destination.a, destination.a);
    }
    if color_mode == 2u {
        return vec4<f32>((source.rgb * source.a + vec3<f32>(1.0 - source.a)) * destination.rgb * destination.a, destination.a);
    }
    return alpha_blend(color_blend(source.rgb, destination.rgb, color_mode), source, destination, alpha_mode);
}

@fragment
fn fs_draw(in: VertexOut) -> @location(0) vec4<f32> {
    let sampled = textureSample(source_tex, source_sampler, in.uv);
    let multiplied = sampled.rgb * u.multiply_color.rgb;
    let base = select(multiplied, sampled.rgb, u.flags.y != 0u);
    let rgb = multiplied + u.screen_color.rgb * sampled.a - base * u.screen_color.rgb;
    let factor = u.opacity.x * mask_alpha(in);
    let alpha = sampled.a * factor;
    if u.blend_mode.x == 1u {
        return vec4<f32>(rgb * factor, 0.0);
    }
    if u.blend_mode.x == 2u {
        return vec4<f32>(rgb * factor + vec3<f32>(1.0 - alpha), 1.0);
    }
    return vec4<f32>(rgb * factor, alpha);
}

@fragment
fn fs_extended_draw(in: VertexOut) -> @location(0) vec4<f32> {
    let raw_sample = textureSample(source_tex, source_sampler, in.uv);
    var source: vec4<f32>;
    if u.flags.y != 0u {
        var composited = vec4<f32>(
            raw_sample.rgb * u.multiply_color.rgb
                + u.screen_color.rgb * raw_sample.a
                - raw_sample.rgb * u.screen_color.rgb,
            raw_sample.a,
        );
        composited *= u.opacity.x * mask_alpha(in);
        source = to_straight(composited);
    } else {
        source = to_straight(raw_sample);
        source = vec4<f32>(source.rgb * u.multiply_color.rgb, source.a);
        source = vec4<f32>(
            source.rgb + u.screen_color.rgb - source.rgb * u.screen_color.rgb,
            source.a,
        );
        source.a *= u.opacity.x * mask_alpha(in);
    }
    let dims = textureDimensions(destination_tex);
    let pixel = min(vec2<u32>(in.position.xy), dims - vec2<u32>(1u, 1u));
    let destination = to_straight(textureLoad(destination_tex, pixel, 0));
    return composite_blend(source, destination, u.blend_mode.x, u.alpha_mode.x);
}

@fragment
fn fs_mask(in: VertexOut) -> @location(0) vec4<f32> {
    return vec4<f32>(0.0, 0.0, 0.0, textureSample(source_tex, source_sampler, in.uv).a);
}

@fragment
fn fs_present(in: VertexOut) -> @location(0) vec4<f32> {
    return textureSample(source_tex, source_sampler, in.uv);
}
