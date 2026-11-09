struct Vertex { float4 position : SV_Position; float2 uv : TEXCOORD0; };
Vertex vertex(uint id : SV_VertexID) {
    Vertex v;
    v.uv = float2((id << 1) & 2, id & 2);
    v.position = float4(v.uv * float2(2, -2) + float2(-1, 1), 0, 1);
    return v;
}
Texture2D<float4> image : register(t0);
SamplerState linear_sampler : register(s0);
float4 fragment(Vertex v) : SV_Target { return image.Sample(linear_sampler, v.uv); }
