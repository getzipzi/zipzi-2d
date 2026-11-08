// Mesh overlay instance format and premultiplied output match overlay.metal.
StructuredBuffer<float4> instances : register(t0);
cbuffer Uniforms : register(b0) {
    float2 viewport; float scale; float padding;
    float2 origin; float2 texture_origin;
    float4 edge; float4 vertex_color; float4 selected_color; float4 restricted_color; float4 hover_color;
};
struct Out { float4 position : SV_Position; float4 color : COLOR0; };
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
    float4 color : COLOR0;
    float2 local : TEXCOORD0;
    nointerpolation float radius : TEXCOORD1;
    nointerpolation float hovered : TEXCOORD2;
    nointerpolation float selected : TEXCOORD3;
    nointerpolation float restricted : TEXCOORD4;
    nointerpolation float4 hover_color : COLOR1;
};
MarkerOut marker_vertex(uint v : SV_VertexID, uint id : SV_InstanceID) {
    float4 marker = instances[id];
    const float2 corner[6] = {float2(-1,-1),float2(1,-1),float2(-1,1),float2(-1,1),float2(1,-1),float2(1,1)};
    bool hovered = (uint(marker.w) & 2) != 0;
    bool restricted = (uint(marker.w) & 4) != 0;
    bool selected = (uint(marker.w) & 1) != 0;
    float radius = marker.z * scale;
    float extent = radius + 1.0;
    float2 local = corner[v] * extent;
    float2 center = (marker.xy + origin) * scale - texture_origin;
    Out base = output(center + local, restricted ? restricted_color : (selected ? selected_color : vertex_color));
    MarkerOut o;
    o.position=base.position; o.color=base.color; o.local=local; o.radius=radius;
    o.hovered=hovered; o.selected=selected; o.restricted=restricted; o.hover_color=hover_color;
    return o;
}
float4 marker_fragment(MarkerOut input) : SV_Target {
    float d = length(input.local);
    float coverage = saturate(input.radius - d + 0.5);
    float4 color = input.color * coverage;
    if (input.selected == 0 || input.restricted > 0 || input.hovered > 0) {
        float inner = saturate(input.radius - scale - d + 0.5);
        float4 border = input.hovered > 0 ? input.hover_color : input.restricted > 0 ? float4(80.0/255.0,80.0/255.0,80.0/255.0,1) : float4(0,0,0,1);
        color = input.color * inner + border * (coverage - inner);
    }
    return color;
}
float4 solid_fragment(Out input) : SV_Target { return input.color; }

struct CanvasInstance { float4 geometry; float4 parameters; float4 color; float4 border; };
StructuredBuffer<CanvasInstance> canvas_instances : register(t1);
struct CanvasOut {
    float4 position : SV_Position;
    float4 color : COLOR0; nointerpolation float4 border : COLOR1;
    float2 local : TEXCOORD0; nointerpolation float2 half_size : TEXCOORD1;
    nointerpolation float radius : TEXCOORD2; nointerpolation float width : TEXCOORD3;
    nointerpolation uint kind : TEXCOORD4;
};
CanvasOut canvas_vertex(uint v : SV_VertexID, uint id : SV_InstanceID) {
    CanvasInstance i = canvas_instances[id];
    uint kind = uint(i.parameters.x);
    float2 a = (i.geometry.xy+origin)*scale-texture_origin;
    float2 b = (i.geometry.zw+origin)*scale-texture_origin;
    float2 local = float2(0,0), half_size = i.geometry.zw*scale;
    float2 position;
    if (kind == 0) {
        float2 d = b-a;
        float2 n = float2(-d.y,d.x)/max(length(d),1e-20)*(i.parameters.y*scale*0.5);
        const uint endpoint[6] = {0,0,1,1,0,1};
        const float side[6] = {1,-1,1,1,-1,-1};
        position = (endpoint[v] ? b : a)+side[v]*n;
    } else if (kind == 1) {
        const float2 corner[6] = {float2(-1,-1),float2(1,-1),float2(-1,1),float2(-1,1),float2(1,-1),float2(1,1)};
        local = corner[v]*(half_size+1.0);
        position = a+local;
    } else {
        float2 c = (i.parameters.yz+origin)*scale-texture_origin;
        position = v == 0 ? a : v == 1 ? b : c;
    }
    float4 color=i.color;
    if (kind==3) color.a *= v==0 ? i.border.x : v==1 ? i.border.y : i.border.z;
    Out base = output(position,color);
    CanvasOut o;
    o.position=base.position; o.color=base.color;
    o.border=float4(i.border.rgb*i.border.a,i.border.a);
    o.local=local; o.half_size=half_size; o.radius=i.parameters.y*scale;
    o.width=i.parameters.z*scale; o.kind=kind;
    return o;
}
float4 canvas_fragment(CanvasOut i) : SV_Target {
    if (i.kind != 1) return i.color;
    float radius=min(i.radius,min(i.half_size.x,i.half_size.y));
    float2 q=abs(i.local)-(i.half_size-radius);
    float d=length(max(q,float2(0,0)))+min(max(q.x,q.y),0.0)-radius;
    float coverage=saturate(0.5-d), inner=saturate(0.5-d-i.width);
    // GPUI composites a translucent border over the marker's background fill.
    float4 blended_border = i.border+i.color*(1.0-i.border.a);
    return i.color*inner+blended_border*(coverage-inner);
}
