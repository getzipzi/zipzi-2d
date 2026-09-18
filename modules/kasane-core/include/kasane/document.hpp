// SPDX-License-Identifier: MIT
#pragma once
#include <kasane/geometry.hpp>
#include <array>
#include <unordered_map>

namespace kasane {
struct Canvas { float width = 0; float height = 0; };
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
};
enum class ChangeKind { none, metadata, positions, structure };
struct ChangeSet {
    ChangeKind kind = ChangeKind::none;
    std::vector<std::string> mesh_ids;
    uint64_t revision = 0;
};
struct EditResult {
    Status status;
    ChangeSet changes;
};

// Source data only. No Godot, textures, render handles, or evaluation outputs.
// Single-threaded ownership. Const references are scoped to the next mutation.
class Document {
public:
    static constexpr uint32_t schema_version = 1;
    Status initialize(std::string id, Canvas canvas);
    bool initialized() const { return !id_.empty(); }
    const std::string &id() const { return id_; }
    Canvas canvas() const { return canvas_; }
    uint64_t revision() const { return revision_; }
    const std::vector<std::string> &mesh_order() const { return mesh_order_; }
    size_t asset_count() const { return assets_.size(); }
    const ImageAsset *get_asset(const std::string &id) const;
    const Mesh *get_mesh(const std::string &id) const;
    EditResult add_asset(ImageAsset asset);
    EditResult create_mesh(Mesh mesh);
    EditResult rename_mesh(const std::string &id, std::string name);
    EditResult set_vertex_positions(const std::string &id,
                                   std::span<const VertexId> vertices,
                                   std::span<const Vec2> positions);
    // Derived dense topology. Does not mutate Document.
    Status render_indices(const std::string &id, std::vector<uint32_t> &out) const;
private:
    std::string id_;
    Canvas canvas_;
    uint64_t revision_ = 0;
    std::unordered_map<std::string, ImageAsset> assets_;
    std::unordered_map<std::string, Mesh> meshes_;
    // Derived identity index, built on creation, not a second topology source.
    std::unordered_map<std::string, std::unordered_map<VertexId, uint32_t>> vertex_slots_;
    std::vector<std::string> mesh_order_;
    bool contains_id(const std::string &id) const;
    EditResult failed(Status status) const;
    EditResult changed(ChangeKind kind, std::vector<std::string> ids = {});
};
bool valid_uuid(const std::string &id);
}
