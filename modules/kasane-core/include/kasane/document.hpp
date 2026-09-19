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
struct VertexPositionUpdate {
    std::string mesh_id;
    std::vector<VertexId> vertex_ids;
    std::vector<Vec2> positions;
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
    bool modified() const { return current_state_id_ != saved_state_id_; }
    bool transaction_active() const { return transaction_active_; }
    bool can_undo() const { return !undo_.empty(); }
    bool can_redo() const { return !redo_.empty(); }
    void mark_saved() { saved_state_id_ = current_state_id_; }
    const std::vector<std::string> &asset_order() const { return asset_order_; }
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
    EditResult apply_vertex_position_updates(std::span<const VertexPositionUpdate> updates);
    EditResult apply_vertex_position_updates_at_revision(std::span<const VertexPositionUpdate> updates,
                                                        uint64_t expected_revision);
    Status begin_transaction();
    Status stage_vertex_positions(VertexPositionUpdate update);
    EditResult commit_transaction();
    Status cancel_transaction();
    EditResult undo();
    EditResult redo();
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
    std::vector<std::string> asset_order_;
    std::vector<std::string> mesh_order_;
    struct PositionDelta {
        std::string mesh_id;
        std::vector<uint32_t> slots;
        std::vector<Vec2> before;
        std::vector<Vec2> after;
    };
    struct HistoryEntry {
        std::vector<PositionDelta> deltas;
        uint64_t before_state_id = 0;
        uint64_t after_state_id = 0;
    };
    std::vector<HistoryEntry> undo_;
    std::vector<HistoryEntry> redo_;
    bool transaction_active_ = false;
    std::vector<VertexPositionUpdate> staged_updates_;
    uint64_t next_state_id_ = 1;
    uint64_t current_state_id_ = 0;
    uint64_t saved_state_id_ = 0;
    bool contains_id(const std::string &id) const;
    bool mutation_blocked() const { return transaction_active_; }
    void advance_state();
    void clear_history();
    EditResult failed(Status status) const;
    EditResult changed(ChangeKind kind, std::vector<std::string> ids = {}, bool clear_history = true);
};
bool valid_uuid(const std::string &id);
}
