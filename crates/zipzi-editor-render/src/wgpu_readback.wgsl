// Convert the renderer's premultiplied UNORM texels to GPUI's straight BGRA.
// Integer arithmetic preserves the previous CPU rounding and saturation exactly.
@group(0) @binding(0) var source: texture_2d<f32>;
@group(0) @binding(1) var<storage, read_write> pixels: array<u32>;

@compute @workgroup_size(8, 8)
fn main(@builtin(global_invocation_id) id: vec3<u32>) {
    let extent = textureDimensions(source);
    if (id.x >= extent.x || id.y >= extent.y) {
        return;
    }
    let rgba = vec4<u32>(round(textureLoad(source, vec2<i32>(id.xy), 0) * 255.0));
    var packed = 0u;
    if (rgba.a != 0u) {
        let rgb = min((rgba.rgb * 255u + rgba.a / 2u) / rgba.a, vec3<u32>(255u));
        packed = rgb.b | (rgb.g << 8u) | (rgb.r << 16u) | (rgba.a << 24u);
    }
    pixels[id.y * extent.x + id.x] = packed;
}
