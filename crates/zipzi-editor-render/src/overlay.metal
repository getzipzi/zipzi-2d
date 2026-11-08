#include <metal_stdlib>
using namespace metal;
struct Uniforms {
    float2 viewport; float scale; float padding;
    float2 origin; float2 texture_origin;
    float4 edge; float4 vertex_color; float4 selected; float4 restricted; float4 hover_color;
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
    float restricted [[flat]];
    float4 hover_color [[flat]];
};
vertex MarkerOut marker_vertex(uint v [[vertex_id]], uint id [[instance_id]],
    const device float4 *markers [[buffer(0)]], constant Uniforms &u [[buffer(1)]]) {
    float4 marker = markers[id];
    const float2 corner[6] = {float2(-1,-1),float2(1,-1),float2(-1,1),float2(-1,1),float2(1,-1),float2(1,1)};
    bool hovered = (uint(marker.w) & 2) != 0;
    bool restricted = (uint(marker.w) & 4) != 0;
    bool selected = (uint(marker.w) & 1) != 0;
    float radius = marker.z * u.scale;
    float extent = radius + 1.0;
    float2 local = corner[v] * extent;
    float2 center = (marker.xy + u.origin) * u.scale - u.texture_origin;
    Out base = output(center + local, restricted ? u.restricted : (selected ? u.selected : u.vertex_color), u);
    return {base.position, base.color, local, radius, u.scale, hovered ? 1.0 : 0.0, selected ? 1.0 : 0.0, restricted ? 1.0 : 0.0, u.hover_color};
}
fragment float4 marker_fragment(MarkerOut input [[stage_in]]) {
    float d = length(input.local);
    float coverage = clamp(input.radius - d + 0.5, 0.0, 1.0);
    float4 color = input.color * coverage;
    if (input.selected == 0 || input.restricted > 0 || input.hovered > 0) {
        // Borders and hover emphasis stay inside the same six-pixel marker.
        float inner = clamp(input.radius - input.scale - d + 0.5, 0.0, 1.0);
        float4 border = input.hovered > 0 ? input.hover_color : input.restricted > 0 ? float4(float3(80.0/255.0), 1.0) : float4(0.0,0.0,0.0,1.0);
        color = input.color * inner + border * (coverage - inner);
    }
    return color;
}
fragment float4 solid_fragment(Out input [[stage_in]]) { return input.color; }

struct CanvasInstance { float4 geometry; float4 parameters; float4 color; float4 border; };
struct CanvasOut {
    float4 position [[position]];
    float4 color; float4 border [[flat]];
    float2 local; float2 half_size [[flat]];
    float radius [[flat]]; float width [[flat]]; uint kind [[flat]];
};
vertex CanvasOut canvas_vertex(uint v [[vertex_id]], uint id [[instance_id]],
    const device CanvasInstance *instances [[buffer(0)]], constant Uniforms &u [[buffer(1)]]) {
    CanvasInstance i = instances[id];
    uint kind = uint(i.parameters.x);
    float2 a = (i.geometry.xy + u.origin) * u.scale - u.texture_origin;
    float2 b = (i.geometry.zw + u.origin) * u.scale - u.texture_origin;
    float2 local = float2(0), half_size = i.geometry.zw * u.scale;
    float2 position;
    if (kind == 0) {
        float2 d = b-a;
        float2 n = float2(-d.y,d.x) / max(length(d), 1e-20) * (i.parameters.y*u.scale*0.5);
        const uint endpoint[6] = {0,0,1,1,0,1};
        const float side[6] = {1,-1,1,1,-1,-1};
        position = (endpoint[v] ? b : a) + side[v]*n;
    } else if (kind == 1) {
        const float2 corner[6] = {float2(-1,-1),float2(1,-1),float2(-1,1),float2(-1,1),float2(1,-1),float2(1,1)};
        local = corner[v] * (half_size+1.0);
        position = a+local;
    } else {
        float2 c = (i.parameters.yz + u.origin)*u.scale-u.texture_origin;
        position = v == 0 ? a : v == 1 ? b : c;
    }
    float4 color=i.color;
    if (kind==3) color.a *= v==0 ? i.border.x : v==1 ? i.border.y : i.border.z;
    Out base = output(position,color,u);
    return {base.position,base.color,float4(i.border.rgb*i.border.a,i.border.a),local,half_size,
        i.parameters.y*u.scale,i.parameters.z*u.scale,kind};
}
fragment float4 canvas_fragment(CanvasOut i [[stage_in]]) {
    if (i.kind != 1) return i.color;
    float radius = min(i.radius,min(i.half_size.x,i.half_size.y));
    float2 q = abs(i.local) - (i.half_size-radius);
    float d = length(max(q,float2(0))) + min(max(q.x,q.y),0.0)-radius;
    float coverage = clamp(0.5-d,0.0,1.0);
    float inner = clamp(0.5-d-i.width,0.0,1.0);
    // GPUI composites a translucent border over the marker's background fill.
    float4 blended_border = i.border + i.color*(1.0-i.border.a);
    return i.color*inner + blended_border*(coverage-inner);
}
