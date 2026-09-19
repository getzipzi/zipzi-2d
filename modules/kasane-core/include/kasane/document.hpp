// SPDX-License-Identifier: MIT
#pragma once
#include <kasane/model.hpp>
#include <kasane/legacy_deformer.hpp>
#include <unordered_map>

namespace kasane {
enum class ChangeKind { none, metadata, positions, structure };
struct ChangeSet {
    ChangeKind kind = ChangeKind::none;
    std::vector<std::string> mesh_ids;
    uint64_t revision = 0;
    std::vector<std::string> object_ids;
};
struct EditResult {
    Status status;
    ChangeSet changes;
    std::vector<std::string> referrers;
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
    static constexpr uint32_t schema_version = 2;
    Status initialize(std::string id, Canvas canvas);
    bool initialized() const { return !id_.empty(); }
    const std::string &id() const { return id_; }
    Canvas canvas() const { return canvas_; }
    uint64_t revision() const { return revision_; }
    bool modified() const { return current_state_id_ != saved_state_id_; }
    bool transaction_active() const { return transaction_active_; }
    void mark_saved() { saved_state_id_ = current_state_id_; }
    const std::vector<std::string> &asset_order() const { return asset_order_; }
    const std::vector<std::string> &mesh_order() const { return mesh_order_; }
    size_t asset_count() const { return assets_.size(); }
    const ImageAsset *get_asset(const std::string &id) const;
    const Mesh *get_mesh(const std::string &id) const;
    const Deformer *get_deformer(const std::string &id) const;
    const std::vector<std::string> &deformer_order() const { return deformer_order_; }
    EditResult create_deformer(Deformer deformer);
    EditResult set_rotation(const std::string &id, Vec2 center, float angle_degrees);
    EditResult set_warp_points(const std::string &id, std::span<const Vec2> points);
    EditResult set_parent(const std::string &id, const std::string &parent, bool organization = false);
    std::string parent_of(const std::string &id, bool organization = false) const;
    // Isolated prototype behavior, never used by the formal evaluator/exporter.
    Status evaluate_legacy_mesh(const std::string &id, std::vector<Vec2> &out) const;
    std::vector<std::string> affected_meshes(const std::string &id) const;
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
    // Restore source data while keeping the live revision monotonic.
    void restore_from(const Document &source);
    EditResult replace_mesh(Mesh mesh);
    const std::vector<std::string> &parameter_order() const { return parameter_order_; }
    const std::vector<std::string> &binding_order() const { return binding_order_; }
    const Parameter *get_parameter(const std::string &) const;
    const MeshBinding *get_binding(const std::string &) const;
    const MeshBinding *binding_for_mesh(const std::string &) const;
    EditResult create_parameter(Parameter);
    EditResult replace_parameter(Parameter);
    EditResult create_binding(MeshBinding);
    EditResult replace_binding(MeshBinding);
    EditResult set_mesh_keyform(const std::string &binding_id, MeshKeyform);
    EditResult replace_mesh_with_keyforms(Mesh, std::span<const VertexMapping>, std::vector<MeshKeyform>);
    std::vector<std::string> references_to(const std::string &) const;
    EditResult erase_object(const std::string &);
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
    std::unordered_map<std::string, Deformer> deformers_;
    std::vector<std::string> deformer_order_;
    std::unordered_map<std::string, Parameter> parameters_;
    std::unordered_map<std::string, MeshBinding> bindings_;
    std::vector<std::string> parameter_order_, binding_order_;
    std::unordered_map<std::string, std::string> deformation_parents_, organization_parents_;
    bool transaction_active_ = false;
    std::vector<VertexPositionUpdate> staged_updates_;
    uint64_t next_state_id_ = 1;
    uint64_t current_state_id_ = 0;
    uint64_t saved_state_id_ = 0;
    bool contains_id(const std::string &id) const;
    bool mutation_blocked() const { return transaction_active_; }
    void advance_state();
    EditResult failed(Status status) const;
    EditResult changed(ChangeKind kind, std::vector<std::string> ids = {}, std::vector<std::string> objects = {});
    Status validate_parameter(const Parameter &) const;
    Status canonicalize_binding(MeshBinding &) const;
};
bool valid_uuid(const std::string &id);
}
