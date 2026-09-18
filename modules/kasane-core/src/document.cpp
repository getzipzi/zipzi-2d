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
void Document::advance_state() { current_state_id_ = next_state_id_++; }
void Document::clear_history() { undo_.clear(); redo_.clear(); }
EditResult Document::changed(ChangeKind kind, std::vector<std::string> ids, bool should_clear_history) {
    advance_state();
    if (should_clear_history) clear_history();
    return {{}, {kind, std::move(ids), ++revision_}};
}
EditResult Document::add_asset(ImageAsset asset) {
    if (mutation_blocked()) return failed(Status::error("TRANSACTION_ACTIVE", "Commit or cancel the active transaction first."));
    if (!initialized()) return failed(Status::error("NOT_INITIALIZED", "Initialize Document first."));
    if (!valid_uuid(asset.id)) return failed(Status::error("INVALID_ID", "Asset ID must be a canonical UUID."));
    if (contains_id(asset.id)) return failed(Status::error("DUPLICATE_ID", "Object ID already exists."));
    if (!asset.width || !asset.height || asset.source.empty())
        return failed(Status::error("INVALID_ASSET", "Asset requires source and positive pixel dimensions."));
    const auto key = asset.id;
    assets_.emplace(key, std::move(asset));
    asset_order_.push_back(key);
    return changed(ChangeKind::metadata);
}
EditResult Document::create_mesh(Mesh mesh) {
    if (mutation_blocked()) return failed(Status::error("TRANSACTION_ACTIVE", "Commit or cancel the active transaction first."));
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
    if (mutation_blocked()) return failed(Status::error("TRANSACTION_ACTIVE", "Commit or cancel the active transaction first."));
    auto it = meshes_.find(id);
    if (it == meshes_.end()) return failed(Status::error("MISSING_MESH", "Mesh does not exist."));
    if (it->second.name == name) return {{}, {ChangeKind::none, {}, revision_}};
    it->second.name = std::move(name);
    return changed(ChangeKind::metadata, {id});
}
EditResult Document::set_vertex_positions(const std::string &id,
                                         std::span<const VertexId> vertices,
                                         std::span<const Vec2> positions) {
    VertexPositionUpdate update{id, {vertices.begin(), vertices.end()}, {positions.begin(), positions.end()}};
    return apply_vertex_position_updates(std::span<const VertexPositionUpdate>(&update, 1));
}
EditResult Document::apply_vertex_position_updates(std::span<const VertexPositionUpdate> updates) {
    if (mutation_blocked()) return failed(Status::error("TRANSACTION_ACTIVE", "Use commit_transaction for staged edits."));
    HistoryEntry entry;
    entry.before_state_id = current_state_id_;
    std::unordered_map<std::string, std::unordered_set<VertexId>> seen;
    std::vector<std::string> changed_meshes;
    std::unordered_set<std::string> changed_mesh_set;
    for (const auto &update : updates) {
        auto mesh_it = meshes_.find(update.mesh_id);
        if (mesh_it == meshes_.end()) return failed(Status::error("MISSING_MESH", "Mesh does not exist."));
        if (update.vertex_ids.size() != update.positions.size())
            return failed(Status::error("INVALID_LENGTH", "IDs and positions must match."));
        if (auto status = validate_positions(update.positions); !status.ok()) return failed(status);
        PositionDelta delta;
        delta.mesh_id = update.mesh_id;
        const auto &lookup = vertex_slots_.at(update.mesh_id);
        for (size_t i = 0; i < update.vertex_ids.size(); ++i) {
            const auto vertex = update.vertex_ids[i];
            if (!seen[update.mesh_id].insert(vertex).second)
                return failed(Status::error("DUPLICATE_VERTEX", "A transaction cannot write a vertex twice."));
            auto slot = lookup.find(vertex);
            if (slot == lookup.end()) return failed(Status::error("MISSING_VERTEX", "Vertex ID does not exist."));
            const auto old = mesh_it->second.base_positions[slot->second];
            if (old == update.positions[i]) continue;
            delta.slots.push_back(slot->second);
            delta.before.push_back(old);
            delta.after.push_back(update.positions[i]);
        }
        if (!delta.slots.empty()) {
            if (changed_mesh_set.insert(update.mesh_id).second) changed_meshes.push_back(update.mesh_id);
            entry.deltas.push_back(std::move(delta));
        }
    }
    if (entry.deltas.empty()) return {{}, {ChangeKind::none, {}, revision_}};
    for (const auto &delta : entry.deltas) {
        auto &positions = meshes_.at(delta.mesh_id).base_positions;
        for (size_t i = 0; i < delta.slots.size(); ++i) positions[delta.slots[i]] = delta.after[i];
    }
    auto result = changed(ChangeKind::positions, std::move(changed_meshes), false);
    entry.after_state_id = current_state_id_;
    undo_.push_back(std::move(entry));
    constexpr size_t history_limit = 128;
    if (undo_.size() > history_limit) undo_.erase(undo_.begin());
    redo_.clear();
    return result;
}
Status Document::begin_transaction() {
    if (!initialized()) return Status::error("NOT_INITIALIZED", "Initialize Document first.");
    if (transaction_active_) return Status::error("TRANSACTION_ACTIVE", "A transaction is already active.");
    transaction_active_ = true;
    staged_updates_.clear();
    return {};
}
Status Document::stage_vertex_positions(VertexPositionUpdate update) {
    if (!transaction_active_) return Status::error("NO_TRANSACTION", "Begin a transaction first.");
    if (update.vertex_ids.size() != update.positions.size())
        return Status::error("INVALID_LENGTH", "IDs and positions must match.");
    if (auto status = validate_positions(update.positions); !status.ok()) return status;
    if (!get_mesh(update.mesh_id)) return Status::error("MISSING_MESH", "Mesh does not exist.");
    staged_updates_.push_back(std::move(update));
    return {};
}
EditResult Document::commit_transaction() {
    if (!transaction_active_) return failed(Status::error("NO_TRANSACTION", "Begin a transaction first."));
    transaction_active_ = false;
    auto updates = std::move(staged_updates_);
    staged_updates_.clear();
    return apply_vertex_position_updates(updates);
}
Status Document::cancel_transaction() {
    if (!transaction_active_) return Status::error("NO_TRANSACTION", "No transaction is active.");
    staged_updates_.clear();
    transaction_active_ = false;
    return {};
}
EditResult Document::undo() {
    if (mutation_blocked()) return failed(Status::error("TRANSACTION_ACTIVE", "Commit or cancel the active transaction first."));
    if (undo_.empty()) return failed(Status::error("NOTHING_TO_UNDO", "There is no committed edit to undo."));
    auto entry = std::move(undo_.back());
    undo_.pop_back();
    std::vector<std::string> ids;
    std::unordered_set<std::string> unique;
    for (const auto &delta : entry.deltas) {
        auto &positions = meshes_.at(delta.mesh_id).base_positions;
        for (size_t i = 0; i < delta.slots.size(); ++i) positions[delta.slots[i]] = delta.before[i];
        if (unique.insert(delta.mesh_id).second) ids.push_back(delta.mesh_id);
    }
    current_state_id_ = entry.before_state_id;
    redo_.push_back(std::move(entry));
    return {{}, {ChangeKind::positions, std::move(ids), ++revision_}};
}
EditResult Document::redo() {
    if (mutation_blocked()) return failed(Status::error("TRANSACTION_ACTIVE", "Commit or cancel the active transaction first."));
    if (redo_.empty()) return failed(Status::error("NOTHING_TO_REDO", "There is no undone edit to redo."));
    auto entry = std::move(redo_.back());
    redo_.pop_back();
    std::vector<std::string> ids;
    std::unordered_set<std::string> unique;
    for (const auto &delta : entry.deltas) {
        auto &positions = meshes_.at(delta.mesh_id).base_positions;
        for (size_t i = 0; i < delta.slots.size(); ++i) positions[delta.slots[i]] = delta.after[i];
        if (unique.insert(delta.mesh_id).second) ids.push_back(delta.mesh_id);
    }
    current_state_id_ = entry.after_state_id;
    undo_.push_back(std::move(entry));
    return {{}, {ChangeKind::positions, std::move(ids), ++revision_}};
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
