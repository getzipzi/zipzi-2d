// Mesh overlay instance format and premultiplied output match overlay.metal.
StructuredBuffer<float4> instances : register(t0);
cbuffer Uniforms : register(b0) {
    float2 viewport; float scale; float padding;
    float2 origin; float2 texture_origin;
    float4 edge; float4 vertex_color; float4 selected_color; float4 restricted_color;
};
struct Out { float4 position : SV_Position; nointerpolation float4 color : COLOR0; };
Out output(float2 p, float4 color) {
    Out o;
    o.position = float4(p / viewport * float2(2,-2) + float2(-1,1),0,1);
    o.color = float4(color.rgb * color.a, color.a);
    return o;
}
Out edge_vertex(uint v : SV_VertexID, uint id : SV_InstanceID) {
    float2 a = (instances[id].xy + origin) * scale - texture_origin;
    float2 b = (instances[id].zw + origin) * scale - texture_origin;
    float2 d = b-a; float len = dot(d,d);
    float2 n = len > 0 ? float2(-d.y,d.x) / sqrt(len) * (0.5*scale) : float2(0,0);
    const uint endpoint[6] = {0,0,1,1,0,1};
    const float side[6] = {1,-1,1,1,-1,-1};
    return output((endpoint[v] ? b : a) + side[v]*n, edge);
}
struct MarkerOut {
    float4 position : SV_Position;
    nointerpolation float4 color : COLOR0;
    float2 local : TEXCOORD0;
    nointerpolation float radius : TEXCOORD1;
    nointerpolation float hovered : TEXCOORD2;
    nointerpolation float selected : TEXCOORD3;
    nointerpolation float restricted : TEXCOORD4;
};
MarkerOut marker_vertex(uint v : SV_VertexID, uint id : SV_InstanceID) {
    float4 marker = instances[id];
    const float2 corner[6] = {float2(-1,-1),float2(1,-1),float2(-1,1),float2(-1,1),float2(1,-1),float2(1,1)};
    bool hovered = (uint(marker.w) & 2) != 0;
    bool restricted = (uint(marker.w) & 4) != 0;
    bool selected = (uint(marker.w) & 1) != 0;
    float radius = marker.z * scale;
    float extent = radius + (hovered ? 2.0 * scale : 0.0) + 1.0;
    float2 local = corner[v] * extent;
    float2 center = (marker.xy + origin) * scale - texture_origin;
    Out base = output(center + local, restricted ? restricted_color : (selected ? selected_color : vertex_color));
    MarkerOut o;
    o.position=base.position; o.color=base.color; o.local=local; o.radius=radius;
    o.hovered=hovered; o.selected=selected; o.restricted=restricted;
    return o;
}
float4 marker_fragment(MarkerOut input) : SV_Target {
    float d = length(input.local);
    float coverage = saturate(input.radius - d + 0.5);
    float4 color = input.color * coverage;
    if (input.selected == 0 || input.restricted > 0) {
        float inner = saturate(input.radius - scale - d + 0.5);
        float4 border = input.restricted > 0 ? float4(80.0/255.0,80.0/255.0,80.0/255.0,1) : float4(0,0,0,1);
        color = input.color * inner + border * (coverage - inner);
    }
    if (input.hovered > 0) {
        float outer = input.radius + 2.0 * scale;
        float ring = saturate(outer - d + 0.5) * saturate(d - (outer - scale) + 0.5);
        color += float4(1,1,1,1) * ring * (1.0 - coverage);
    }
    return color;
}
float4 solid_fragment(Out input) : SV_Target { return input.color; }
