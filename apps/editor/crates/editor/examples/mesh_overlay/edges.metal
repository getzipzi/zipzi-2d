#include <metal_stdlib>
using namespace metal;
struct Uniforms {
    float2 viewport;
    float2 stretch;
    float2 translation;
    float half_width;
    uint frame;
    float4 color;
    float4 clip;
};
struct Out {
    float4 position [[position]];
    float clip_distance [[clip_distance]][4];
};
vertex Out edge_vertex(uint v [[vertex_id]], uint edge [[instance_id]],
    const device float2 *positions [[buffer(0)]],
    const device uint2 *indices [[buffer(1)]],
    constant Uniforms &u [[buffer(2)]]) {
    uint2 ids = indices[edge];
    float2 a = positions[ids.x] * u.stretch + u.translation;
    float2 b = positions[ids.y] * u.stretch + u.translation;
    float2 d = b - a;
    float length_squared = dot(d, d);
    float2 n = length_squared > 0 ? float2(-d.y, d.x) / sqrt(length_squared) * u.half_width : float2(0);
    // Two butt-capped triangles per independent edge, matching GPUI strokes.
    const uint endpoint[6] = {0, 0, 1, 1, 0, 1};
    const float side[6] = {1, -1, 1, 1, -1, -1};
    float2 p = (endpoint[v] ? b : a) + side[v] * n;
    return {float4(p * float2(2.0 / u.viewport.x, -2.0 / u.viewport.y) + float2(-1, 1), 0, 1),
        {p.x - u.clip.x, u.clip.z - p.x, p.y - u.clip.y, u.clip.w - p.y}};
}
fragment float4 edge_fragment(constant Uniforms &u [[buffer(0)]]) {
    return float4(u.color.rgb * u.color.a, u.color.a);
}

// Square markers match GPUI's borderless/unrounded quad fast path. Render after
// resolving the MSAA edge pass, at sample count 1, preserving insertion order.
struct HandleOut {
    float4 position [[position]];
    float4 color [[flat]];
    float clip_distance [[clip_distance]][4];
};
vertex HandleOut handle_vertex(uint v [[vertex_id]], uint id [[instance_id]],
    const device float2 *positions [[buffer(0)]], constant Uniforms &u [[buffer(2)]],
    constant uint &count [[buffer(3)]]) {
    // Match the CPU logical->physical rounding used by the GPUI reference.
    float2 center = (positions[id] * u.stretch + u.translation) / 2.0;
    float radius = id == (u.frame * 17) % count ? 4.0 : 3.0;
    const float2 corners[6] = {float2(0,0), float2(1,0), float2(0,1),
        float2(0,1), float2(1,0), float2(1,1)};
    float2 origin = (center - radius) * 2.0;
    float2 p = origin + corners[v] * (radius * 4.0);
    float4 color = id % 37 == 0 ? float4(1, 64.0/255, 96.0/255, 1) : float4(240.0/255,240.0/255,240.0/255,1);
    return {float4(p * float2(2.0 / u.viewport.x, -2.0 / u.viewport.y) + float2(-1, 1), 0, 1), color,
        {p.x - u.clip.x, u.clip.z - p.x, p.y - u.clip.y, u.clip.w - p.y}};
}
struct HandleInput { float4 position [[position]]; float4 color [[flat]]; };
fragment float4 handle_fragment(HandleInput input [[stage_in]]) { return input.color; }
