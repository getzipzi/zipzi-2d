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
struct MarkerOut {
    float4 position [[position]];
    float4 color [[flat]];
    float2 local;
    float radius [[flat]];
    float scale [[flat]];
    float hovered [[flat]];
    float selected [[flat]];
};
vertex MarkerOut marker_vertex(uint v [[vertex_id]], uint id [[instance_id]],
    const device float4 *markers [[buffer(0)]], constant Uniforms &u [[buffer(1)]]) {
    float4 marker = markers[id];
    const float2 corner[6] = {float2(-1,-1),float2(1,-1),float2(-1,1),float2(-1,1),float2(1,-1),float2(1,1)};
    bool hovered = marker.w >= 2;
    bool selected = (uint(marker.w) & 1) != 0;
    float radius = marker.z * u.scale;
    float extent = radius + (hovered ? 2.0 * u.scale : 0.0) + 1.0;
    float2 local = corner[v] * extent;
    float2 center = (marker.xy + u.origin) * u.scale - u.texture_origin;
    Out base = output(center + local, selected ? u.selected : u.vertex_color, u);
    return {base.position, base.color, local, radius, u.scale, hovered ? 1.0 : 0.0, selected ? 1.0 : 0.0};
}
fragment float4 marker_fragment(MarkerOut input [[stage_in]]) {
    float d = length(input.local);
    float coverage = clamp(input.radius - d + 0.5, 0.0, 1.0);
    float4 color = input.color * coverage;
    if (input.selected == 0) {
        // Unselected mesh points have a 1 logical pixel black border inside
        // the same silhouette as the solid selected point.
        float inner = clamp(input.radius - input.scale - d + 0.5, 0.0, 1.0);
        color = input.color * inner + float4(0.0, 0.0, 0.0, coverage - inner);
    }
    if (input.hovered > 0) {
        float outer = input.radius + 2.0 * input.scale;
        float ring = clamp(outer - d + 0.5, 0.0, 1.0)
            * clamp(d - (outer - input.scale) + 0.5, 0.0, 1.0);
        // Hollow hover rings are always white, independent of selection color.
        color += float4(1.0) * ring * (1.0 - coverage);
    }
    return color;
}
fragment float4 solid_fragment(Out input [[stage_in]]) { return input.color; }
