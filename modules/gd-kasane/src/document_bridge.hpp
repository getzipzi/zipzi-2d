// SPDX-License-Identifier: MIT
#pragma once
#include "mesh_view.hpp"
#include "mesh_data.hpp"
#include <godot_cpp/classes/node2d.hpp>

namespace kasane_gd {
class KasaneDocumentState : public godot::RefCounted {
    GDCLASS(KasaneDocumentState, godot::RefCounted)
    friend class KasaneDocumentBridge;
    kasane::Document document;
    std::unordered_map<std::string, godot::Ref<godot::Texture2D>> textures;
    uint64_t owner = 0;
    uint64_t generation = 0;
protected:
    static void _bind_methods() {}
};
class KasaneDocumentBridge : public godot::Node2D {
    GDCLASS(KasaneDocumentBridge, godot::Node2D)
    kasane::Document document_;
    uint64_t generation_ = 1;
    godot::String saved_content_;
    godot::String content_signature() const;
    godot::Dictionary write_mesh(const godot::Dictionary &description, bool replace);
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
    godot::Dictionary replace_mesh(const godot::Dictionary &description);
    godot::Ref<KasaneMeshData> get_mesh(const godot::String &id) const;
    godot::Ref<KasaneDocumentState> capture_state() const;
    godot::Dictionary restore_state(const godot::Ref<KasaneDocumentState> &state);
    uint64_t generation() const { return generation_; }
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
