// SPDX-License-Identifier: MIT
#include <kasane/document.hpp>
#include <cmath>
#include <unordered_set>

namespace kasane {
bool valid_uuid(const std::string &id) {
    if (id.size() != 36) return false;
    bool nonzero = false;
    for (size_t i = 0; i < id.size(); ++i) {
        if (i == 8 || i == 13 || i == 18 || i == 23) {
            if (id[i] != '-') return false;
        } else {
            if (!((id[i] >= '0' && id[i] <= '9') || (id[i] >= 'a' && id[i] <= 'f'))) return false;
            nonzero |= id[i] != '0';
        }
    }
    return nonzero;
}
Status Document::initialize(std::string id, Canvas canvas) {
    if (initialized()) return Status::error("ALREADY_INITIALIZED", "Create a new Document to open another model.");
    if (!valid_uuid(id)) return Status::error("INVALID_ID", "Use a nonzero canonical lowercase UUID.");
    if (!std::isfinite(canvas.width) || !std::isfinite(canvas.height) || canvas.width <= 0 || canvas.height <= 0)
        return Status::error("INVALID_CANVAS", "Canvas dimensions must be finite and positive.");
    id_ = std::move(id);
    canvas_ = canvas;
    return {};
}
bool Document::contains_id(const std::string &id) const {
    return id == id_ || assets_.contains(id) || meshes_.contains(id);
}
const ImageAsset *Document::get_asset(const std::string &id) const {
    auto it = assets_.find(id);
    return it == assets_.end() ? nullptr : &it->second;
}
const Mesh *Document::get_mesh(const std::string &id) const {
    auto it = meshes_.find(id);
    return it == meshes_.end() ? nullptr : &it->second;
}
EditResult Document::failed(Status status) const { return {std::move(status), {ChangeKind::none, {}, revision_}}; }
EditResult Document::changed(ChangeKind kind, std::vector<std::string> ids) {
    return {{}, {kind, std::move(ids), ++revision_}};
}
EditResult Document::add_asset(ImageAsset asset) {
    if (!initialized()) return failed(Status::error("NOT_INITIALIZED", "Initialize Document first."));
    if (!valid_uuid(asset.id)) return failed(Status::error("INVALID_ID", "Asset ID must be a canonical UUID."));
    if (contains_id(asset.id)) return failed(Status::error("DUPLICATE_ID", "Object ID already exists."));
    if (!asset.width || !asset.height || asset.source.empty())
        return failed(Status::error("INVALID_ASSET", "Asset requires source and positive pixel dimensions."));
    const auto key = asset.id;
    assets_.emplace(key, std::move(asset));
    return changed(ChangeKind::metadata);
}
EditResult Document::create_mesh(Mesh mesh) {
    if (!initialized()) return failed(Status::error("NOT_INITIALIZED", "Initialize Document first."));
    if (!valid_uuid(mesh.id)) return failed(Status::error("INVALID_ID", "Mesh ID must be a canonical UUID."));
    if (contains_id(mesh.id)) return failed(Status::error("DUPLICATE_ID", "Object ID already exists."));
    if (!get_asset(mesh.texture_asset_id)) return failed(Status::error("MISSING_ASSET", "Texture asset does not exist."));
    if (mesh.vertex_ids.size() != mesh.base_positions.size())
        return failed(Status::error("INVALID_LENGTH", "Vertex IDs must match positions."));
    std::unordered_map<VertexId, uint32_t> slots;
    for (size_t i = 0; i < mesh.vertex_ids.size(); ++i) {
        if (!slots.emplace(mesh.vertex_ids[i], static_cast<uint32_t>(i)).second)
            return failed(Status::error("DUPLICATE_VERTEX", "Vertex IDs must be unique within a mesh."));
    }
    std::vector<uint32_t> indices;
    for (const auto &triangle : mesh.triangles) {
        for (auto vertex : triangle) {
            auto it = slots.find(vertex);
            if (it == slots.end()) return failed(Status::error("MISSING_VERTEX", "Triangle references an unknown vertex ID."));
            indices.push_back(it->second);
        }
    }
    if (auto status = validate_render_mesh(mesh.base_positions, mesh.uvs, indices); !status.ok()) return failed(status);
    const auto key = mesh.id;
    meshes_.emplace(key, std::move(mesh));
    vertex_slots_.emplace(key, std::move(slots));
    mesh_order_.push_back(key);
    return changed(ChangeKind::structure, {key});
}
EditResult Document::rename_mesh(const std::string &id, std::string name) {
    auto it = meshes_.find(id);
    if (it == meshes_.end()) return failed(Status::error("MISSING_MESH", "Mesh does not exist."));
    if (it->second.name == name) return {{}, {ChangeKind::none, {}, revision_}};
    it->second.name = std::move(name);
    return changed(ChangeKind::metadata, {id});
}
EditResult Document::set_vertex_positions(const std::string &id,
                                         std::span<const VertexId> vertices,
                                         std::span<const Vec2> positions) {
    auto it = meshes_.find(id);
    if (it == meshes_.end()) return failed(Status::error("MISSING_MESH", "Mesh does not exist."));
    if (vertices.size() != positions.size()) return failed(Status::error("INVALID_LENGTH", "IDs and positions must match."));
    if (auto status = validate_positions(positions); !status.ok()) return failed(status);
    auto &mesh = it->second;
    const auto &lookup = vertex_slots_.at(id);
    std::unordered_set<VertexId> seen;
    std::vector<size_t> slots;
    bool different = false;
    for (size_t i = 0; i < vertices.size(); ++i) {
        if (!seen.insert(vertices[i]).second) return failed(Status::error("DUPLICATE_VERTEX", "A batch cannot write a vertex twice."));
        auto slot = lookup.find(vertices[i]);
        if (slot == lookup.end()) return failed(Status::error("MISSING_VERTEX", "Vertex ID does not exist."));
        slots.push_back(slot->second);
        different |= !(mesh.base_positions[slot->second] == positions[i]);
    }
    if (!different) return {{}, {ChangeKind::none, {}, revision_}};
    for (size_t i = 0; i < slots.size(); ++i) mesh.base_positions[slots[i]] = positions[i];
    return changed(ChangeKind::positions, {id});
}
Status Document::render_indices(const std::string &id, std::vector<uint32_t> &out) const {
    const auto *mesh = get_mesh(id);
    if (!mesh) return Status::error("MISSING_MESH", "Mesh does not exist.");
    const auto &slots = vertex_slots_.at(id);
    std::vector<uint32_t> next;
    next.reserve(mesh->triangles.size() * 3);
    for (const auto &triangle : mesh->triangles)
        for (auto vertex : triangle) next.push_back(slots.at(vertex));
    out = std::move(next);
    return {};
}
}
