# zipzi-moc3

MOC3 binary format support for encoding, decoding, inspection, and model import.

Raw decoders/importers preserve runtime texture slots and UVs. Project/SDK session
imports additionally unpack those pages into independent Mesh images; see
[project import artwork](../zipzi-project/README.md#import-artwork).

Import converts model-space lengths by pixels-per-unit (PPU), retains
Rotation-local lengths, and leaves Warp-local UVs dimensionless. The runtime
unit bake is undone in the first Rotation scale on each branch, including below Warps;
deeper Rotation scales remain relative. Ordinary and BlendShape keyforms use
the same conversion. `Transform.rotation_scale_unit` retains the scale-channel
conversion so native BlendShape clamp limits remain equivalent after
normalization. Native projects preserve this value. Export inverts both scale
channels and the accumulated coordinate-unit conversions, including mixed
imported/authored hierarchies and changed export PPU.
