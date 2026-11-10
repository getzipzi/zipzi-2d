cbuffer DrawConstants : register(b0) {
    float4 clip_x;
    float4 clip_y;
    float4 multiply_color;
    float4 screen_color;
    float4 mask_x;
    float4 mask_y;
    float4 options; // opacity, masked, inverted, legacy blend mode
    float4 modes; // group composite, raw color mode, raw alpha mode
};

Texture2D source_texture : register(t0);
Texture2D mask_texture : register(t1);
Texture2D destination_texture : register(t2);
SamplerState source_sampler : register(s0);
SamplerState mask_sampler : register(s1);

struct RasterVertex {
    float4 position : SV_Position;
    float2 uv : TEXCOORD0;
    float2 mask_uv : TEXCOORD1;
};

RasterVertex vs_draw(float2 position : POSITION, float2 uv : TEXCOORD0) {
    RasterVertex vertex;
    vertex.position = float4(dot(clip_x.xy, position) + clip_x.w,
                             dot(clip_y.xy, position) + clip_y.w, 0.0, 1.0);
    vertex.uv = uv;
    vertex.mask_uv = float2(dot(mask_x.xy, position) + mask_x.z,
                            dot(mask_y.xy, position) + mask_y.z);
    return vertex;
}

float mask_alpha(RasterVertex vertex) {
    float coverage = 1.0;
    if (options.y != 0.0) {
        float alpha = 0.0;
        if (all(vertex.mask_uv >= 0.0) && all(vertex.mask_uv <= 1.0)) {
            alpha = mask_texture.SampleLevel(mask_sampler, vertex.mask_uv, 0).a;
        }
        coverage = options.z == 0.0 ? alpha : 1.0 - alpha;
    }
    return coverage;
}

float4 ps_draw(RasterVertex vertex) : SV_Target {
    float4 source = source_texture.Sample(source_sampler, vertex.uv);
    float3 multiplied = source.rgb * multiply_color.rgb;
    float3 base = modes.x != 0.0 ? source.rgb : multiplied;
    float3 rgb = multiplied + screen_color.rgb * (source.a - base);
    float factor = options.x * mask_alpha(vertex);
    float alpha = source.a * factor;
    if (options.w == 1.0) {
        return float4(rgb * factor, 0.0);
    }
    if (options.w == 2.0) {
        return float4(rgb * factor + (1.0 - alpha), 1.0);
    }
    return float4(rgb * factor, alpha);
}

float4 ps_mask(RasterVertex vertex) : SV_Target {
    return float4(0.0, 0.0, 0.0, source_texture.Sample(source_sampler, vertex.uv).a);
}

// The raw blend equations match zipzi-render-wgpu/src/zipzi.wgsl.
float4 to_straight(float4 value) {
    return abs(value.a) < 0.00001 ? 0.0 : float4(value.rgb / value.a, value.a);
}

float color_burn(float source, float destination) {
    if (abs(destination - 1.0) < 0.000001) return 1.0;
    if (abs(source) < 0.000001) return 0.0;
    return 1.0 - min(1.0, (1.0 - destination) / source);
}

float color_dodge(float source, float destination) {
    if (destination <= 0.0) return 0.0;
    if (source >= 1.0) return 1.0;
    return min(1.0, destination / (1.0 - source));
}

float soft_light(float source, float destination) {
    if (source <= 0.5) return destination - (1.0 - 2.0 * source) * destination * (1.0 - destination);
    float curve = destination <= 0.25
        ? ((16.0 * destination - 12.0) * destination + 3.0) * destination
        : sqrt(max(destination, 0.0)) - destination;
    return destination + (2.0 * source - 1.0) * curve;
}

float luma(float3 value) {
    return dot(value, float3(0.30, 0.59, 0.11));
}

float saturation(float3 value) {
    return max(value.r, max(value.g, value.b)) - min(value.r, min(value.g, value.b));
}

float3 clip_color(float3 value) {
    float lum = luma(value);
    float hi = max(value.r, max(value.g, value.b));
    float lo = min(value.r, min(value.g, value.b));
    if (lo < 0.0 && lum != lo) value = lum + (value - lum) * lum / (lum - lo);
    if (hi > 1.0 && lum != hi) value = lum + (value - lum) * (1.0 - lum) / (hi - lum);
    return value;
}

float3 set_luma(float3 value, float lum) {
    return clip_color(value + (lum - luma(value)));
}

float3 set_saturation(float3 value, float sat) {
    float hi = max(value.r, max(value.g, value.b));
    float lo = min(value.r, min(value.g, value.b));
    float mid = value.r + value.g + value.b - hi - lo;
    float out_hi = lo < hi ? sat : 0.0;
    float out_mid = lo < hi ? (mid - lo) * sat / max(hi - lo, 0.000001) : 0.0;
    if (value.r == hi) return value.b < value.g ? float3(out_hi, out_mid, 0.0) : float3(out_hi, 0.0, out_mid);
    if (value.g == hi) return value.b < value.r ? float3(out_mid, out_hi, 0.0) : float3(0.0, out_hi, out_mid);
    return value.g < value.r ? float3(out_mid, 0.0, out_hi) : float3(0.0, out_mid, out_hi);
}

float3 color_blend(float3 s, float3 d, uint mode) {
    switch (mode) {
        case 0: return s;
        case 1:
        case 3: return min(s + d, 1.0);
        case 2:
        case 6: return s * d;
        case 4: return s + d;
        case 5: return min(s, d);
        case 7: return float3(color_burn(s.r, d.r), color_burn(s.g, d.g), color_burn(s.b, d.b));
        case 8: return max(0.0, s + d - 1.0);
        case 9: return max(s, d);
        case 10: return s + d - s * d;
        case 11: return float3(color_dodge(s.r, d.r), color_dodge(s.g, d.g), color_dodge(s.b, d.b));
        case 12: return lerp(2.0 * s * d, 1.0 - 2.0 * (1.0 - s) * (1.0 - d), step(0.5, d));
        case 13: return float3(soft_light(s.r, d.r), soft_light(s.g, d.g), soft_light(s.b, d.b));
        case 14: return lerp(2.0 * s * d, 1.0 - 2.0 * (1.0 - s) * (1.0 - d), step(0.5, s));
        case 15: return lerp(max(0.0, 2.0 * s + d - 1.0), min(1.0, 2.0 * (s - 0.5) + d), step(0.5, s));
        case 16: return set_luma(set_saturation(s, saturation(d)), luma(d));
        default: return set_luma(s, luma(d));
    }
}

float4 composite_blend(float4 s, float4 d, uint color_mode, uint alpha_mode) {
    if (color_mode == 1) return float4(s.rgb * s.a + d.rgb * d.a, d.a);
    if (color_mode == 2) return float4((s.rgb * s.a + 1.0 - s.a) * d.rgb * d.a, d.a);
    float3 weights;
    switch (alpha_mode) {
        case 1: weights = float3(s.a * d.a, 0.0, d.a * (1.0 - s.a)); break;
        case 2: weights = float3(0.0, 0.0, d.a * (1.0 - s.a)); break;
        case 3: weights = float3(min(s.a, d.a), max(s.a - d.a, 0.0), max(d.a - s.a, 0.0)); break;
        case 4: weights = float3(max(s.a + d.a - 1.0, 0.0), min(s.a, 1.0 - d.a), min(d.a, 1.0 - s.a)); break;
        default: weights = float3(s.a * d.a, s.a * (1.0 - d.a), d.a * (1.0 - s.a)); break;
    }
    float3 color = color_blend(s.rgb, d.rgb, color_mode);
    return float4(color * weights.x + s.rgb * weights.y + d.rgb * weights.z,
                  weights.x + weights.y + weights.z);
}

float4 ps_extended_draw(RasterVertex vertex) : SV_Target {
    float4 sampled = source_texture.Sample(source_sampler, vertex.uv);
    float factor = options.x * mask_alpha(vertex);
    float4 source;
    if (modes.x != 0.0) {
        float3 rgb = sampled.rgb * multiply_color.rgb + screen_color.rgb * (sampled.a - sampled.rgb);
        source = to_straight(float4(rgb, sampled.a) * factor);
    } else {
        source = to_straight(sampled);
        source.rgb *= multiply_color.rgb;
        source.rgb += screen_color.rgb - source.rgb * screen_color.rgb;
        source.a *= factor;
    }
    float4 destination = to_straight(destination_texture.Load(int3(int2(vertex.position.xy), 0)));
    return composite_blend(source, destination, (uint)modes.y, (uint)modes.z);
}
