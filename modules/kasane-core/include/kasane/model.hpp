// SPDX-License-Identifier: MIT
#pragma once
#include <kasane/geometry.hpp>
#include <array>
#include <optional>
namespace kasane {
// Source coordinates: pixels, X right / Y down. Origin is measured from the
// top-left. Runtime coordinates: (x-origin.x)/ppu, (origin.y-y)/ppu.
// Source UVs: (0,0) top-left; runtime UVs: (u, 1-v).
struct Canvas {
    float width = 0; float height = 0;
    Vec2 origin{};
    float pixels_per_unit = 1;
};
struct ImageAsset {
    std::string id;
    std::string name;
    std::string source;
    uint32_t width = 0;
    uint32_t height = 0;
};
struct Mesh {
    std::string id;
    std::string name;
    std::string texture_asset_id;
    std::vector<VertexId> vertex_ids;
    std::vector<Vec2> base_positions;
    std::vector<Vec2> uvs;
    std::vector<std::array<VertexId, 3>> triangles;
    // Independent from name and internal identity. Empty on creation uses id.
    std::string runtime_id;
};
struct Parameter {
    std::string id, runtime_id, name;
    float minimum = -1, maximum = 1, default_value = 0;
    int32_t decimal_places = 6;
};
struct BindingAxis { std::string parameter_id; std::vector<float> keys; };
struct MeshKeyform {
    // Explicit key value per axis, in binding axis order.
    std::vector<float> keys;
    std::vector<Vec2> positions;
};
struct MeshBinding {
    std::string id, mesh_id;
    std::vector<BindingAxis> axes;
    // Stored in canonical Cartesian order: axis 0 varies fastest.
    std::vector<MeshKeyform> keyforms;
};
struct VertexMapping { VertexId new_id; std::optional<VertexId> old_id; };
}
