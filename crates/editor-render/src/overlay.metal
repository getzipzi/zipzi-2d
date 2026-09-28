#include <metal_stdlib>
using namespace metal;
struct Uniforms {
    float2 viewport; float scale; float padding;
    float2 origin; float2 texture_origin;
    float4 edge; float4 vertex_color; float4 selected;
};
struct Out { float4 position [[position]]; float4 color [[flat]]; };
Out output(float2 p, float4 color, constant Uniforms &u) {
    return {float4(p / u.viewport * float2(2,-2) + float2(-1,1),0,1), float4(color.rgb * color.a, color.a)};
}
vertex Out edge_vertex(uint v [[vertex_id]], uint id [[instance_id]],
    const device float4 *edges [[buffer(0)]], constant Uniforms &u [[buffer(1)]]) {
    float2 a = (edges[id].xy + u.origin) * u.scale - u.texture_origin, b = (edges[id].zw + u.origin) * u.scale - u.texture_origin;
    float2 d = b-a; float len = dot(d,d);
    float2 n = len > 0 ? float2(-d.y,d.x) / sqrt(len) * (0.5*u.scale) : float2(0);
    const uint endpoint[6] = {0,0,1,1,0,1};
    const float side[6] = {1,-1,1,1,-1,-1};
    return output((endpoint[v] ? b : a) + side[v]*n, u.edge, u);
}
vertex Out marker_vertex(uint v [[vertex_id]], uint id [[instance_id]],
    const device float4 *markers [[buffer(0)]], constant Uniforms &u [[buffer(1)]]) {
    float4 marker = markers[id];
    const float2 corner[6] = {float2(0,0),float2(1,0),float2(0,1),float2(0,1),float2(1,0),float2(1,1)};
    float2 center = marker.xy + u.origin;
    float2 lo = (center - marker.z) * u.scale;
    float2 hi = (center + marker.z) * u.scale;
    // Match Window::paint_quad snapping, including half-pixel ties toward zero.
    lo = copysign(ceil(abs(lo) - 0.5), lo);
    hi = copysign(ceil(abs(hi) - 0.5), hi);
    float2 p = lo + corner[v] * (hi-lo) - u.texture_origin;
    return output(p, marker.w > 0 ? u.selected : u.vertex_color, u);
}
fragment float4 solid_fragment(Out input [[stage_in]]) { return input.color; }
