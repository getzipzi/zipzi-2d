// SPDX-License-Identifier: MIT
#pragma once
#include "mesh_view.hpp"
#include <godot_cpp/classes/node2d.hpp>

namespace kasane_gd {
class KasaneDocumentBridge : public godot::Node2D {
    GDCLASS(KasaneDocumentBridge, godot::Node2D)
    kasane::Document document_;
    std::unordered_map<std::string, godot::Ref<godot::Texture2D>> textures_;
    std::unordered_map<std::string, KasaneMeshView *> views_;
    godot::Dictionary apply(const kasane::EditResult &edit);
    kasane::Status create_view(const std::string &id);
protected:
    static void _bind_methods();
public:
    godot::Dictionary initialize(const godot::String &id, godot::Vector2 canvas_size);
    godot::Dictionary add_image_asset(const godot::String &id, const godot::String &name,
                                     const godot::String &source, const godot::Ref<godot::Texture2D> &texture);
    godot::Dictionary create_mesh(const godot::Dictionary &description);
    godot::Dictionary set_vertex_positions(const godot::String &mesh_id,
                                          const godot::PackedInt64Array &vertex_ids,
                                          const godot::PackedVector2Array &positions);
    godot::Dictionary rename_mesh(const godot::String &id, const godot::String &name);
    godot::Dictionary begin_transaction();
    godot::Dictionary stage_vertex_positions(const godot::String &mesh_id,
                                             const godot::PackedInt64Array &vertex_ids,
                                             const godot::PackedVector2Array &positions);
    godot::Dictionary commit_transaction();
    godot::Dictionary cancel_transaction();
    godot::Dictionary undo();
    godot::Dictionary redo();
    godot::Dictionary commit_vertex_updates(const godot::Array &updates, int64_t expected_revision);
    godot::Dictionary get_asset_snapshot(const godot::String &id) const;
    godot::Dictionary save_project(const godot::String &path);
    godot::Dictionary open_project(const godot::String &path);
    godot::Dictionary get_mesh_snapshot(const godot::String &id) const;
    godot::Dictionary get_document_summary() const;
    KasaneMeshView *get_mesh_view(const godot::String &id) const;
    godot::Dictionary rebuild_preview();
};
}
