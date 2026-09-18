// SPDX-License-Identifier: MIT
#include "document_bridge.hpp"
#include <godot_cpp/core/class_db.hpp>
#include <godot_cpp/classes/os.hpp>

using namespace godot;
namespace kasane_gd {
void KasaneDocumentBridge::_bind_methods() {
    ClassDB::bind_method(D_METHOD("initialize", "id", "canvas_size"), &KasaneDocumentBridge::initialize);
    ClassDB::bind_method(D_METHOD("add_image_asset", "id", "name", "source", "texture"), &KasaneDocumentBridge::add_image_asset);
    ClassDB::bind_method(D_METHOD("create_mesh", "description"), &KasaneDocumentBridge::create_mesh);
    ClassDB::bind_method(D_METHOD("set_vertex_positions", "mesh_id", "vertex_ids", "positions"), &KasaneDocumentBridge::set_vertex_positions);
    ClassDB::bind_method(D_METHOD("rename_mesh", "id", "name"), &KasaneDocumentBridge::rename_mesh);
    ClassDB::bind_method(D_METHOD("get_mesh_snapshot", "id"), &KasaneDocumentBridge::get_mesh_snapshot);
    ClassDB::bind_method(D_METHOD("get_document_summary"), &KasaneDocumentBridge::get_document_summary);
    ClassDB::bind_method(D_METHOD("get_mesh_view", "id"), &KasaneDocumentBridge::get_mesh_view);
    ClassDB::bind_method(D_METHOD("rebuild_preview"), &KasaneDocumentBridge::rebuild_preview);
}
Dictionary KasaneDocumentBridge::initialize(const String &id, Vector2 size) {
    if (!(OS::get_singleton()->get_thread_caller_id() == OS::get_singleton()->get_main_thread_id())) return error("WRONG_THREAD", "Document bridge requires the main thread.");
    return result(document_.initialize(utf8(id), {static_cast<float>(size.x), static_cast<float>(size.y)}));
}
Dictionary KasaneDocumentBridge::add_image_asset(const String &id, const String &name,
                                                const String &source, const Ref<Texture2D> &texture) {
    if (!(OS::get_singleton()->get_thread_caller_id() == OS::get_singleton()->get_main_thread_id())) return error("WRONG_THREAD", "Document bridge requires the main thread.");
    if (texture.is_null() || texture->get_width() <= 0 || texture->get_height() <= 0)
        return error("INVALID_TEXTURE", "Supply a loaded Texture2D.");
    const auto key = utf8(id);
    auto edit = document_.add_asset({key, utf8(name), utf8(source),
        static_cast<uint32_t>(texture->get_width()), static_cast<uint32_t>(texture->get_height())});
    if (edit.status.ok()) textures_.emplace(key, texture);
    return apply(edit);
}
Dictionary KasaneDocumentBridge::create_mesh(const Dictionary &d) {
    if (!(OS::get_singleton()->get_thread_caller_id() == OS::get_singleton()->get_main_thread_id())) return error("WRONG_THREAD", "Document bridge requires the main thread.");
    const std::pair<const char *, Variant::Type> required[] = {
        {"id", Variant::STRING}, {"name", Variant::STRING}, {"texture_asset_id", Variant::STRING},
        {"vertex_ids", Variant::PACKED_INT64_ARRAY}, {"base_positions", Variant::PACKED_VECTOR2_ARRAY},
        {"uvs", Variant::PACKED_VECTOR2_ARRAY}, {"triangles", Variant::PACKED_INT64_ARRAY}};
    for (const auto &[key, type] : required)
        if (!d.has(key) || d[key].get_type() != type) return error("INVALID_FIELD", "Missing field or incorrect field type; see Document specification.");
    kasane::Mesh mesh;
    mesh.id = utf8(d["id"]); mesh.name = utf8(d["name"]); mesh.texture_asset_id = utf8(d["texture_asset_id"]);
    if (auto s = ids(d["vertex_ids"], mesh.vertex_ids); !s.ok()) return result(s);
    mesh.base_positions = vectors(PackedVector2Array(d["base_positions"]));
    mesh.uvs = vectors(PackedVector2Array(d["uvs"]));
    std::vector<uint32_t> triangles;
    if (auto s = ids(d["triangles"], triangles); !s.ok()) return result(s);
    if (triangles.size() % 3) return error("INVALID_LENGTH", "Triangle vertex IDs must be a multiple of three.");
    for (size_t i = 0; i < triangles.size(); i += 3) mesh.triangles.push_back({triangles[i], triangles[i + 1], triangles[i + 2]});
    return apply(document_.create_mesh(std::move(mesh)));
}
Dictionary KasaneDocumentBridge::set_vertex_positions(const String &mesh_id, const PackedInt64Array &vertex_ids,
                                                     const PackedVector2Array &positions) {
    if (!(OS::get_singleton()->get_thread_caller_id() == OS::get_singleton()->get_main_thread_id())) return error("WRONG_THREAD", "Document bridge requires the main thread.");
    std::vector<uint32_t> vertices;
    if (auto s = ids(vertex_ids, vertices); !s.ok()) return result(s);
    return apply(document_.set_vertex_positions(utf8(mesh_id), vertices, vectors(positions)));
}
Dictionary KasaneDocumentBridge::rename_mesh(const String &id, const String &name) {
    if (!(OS::get_singleton()->get_thread_caller_id() == OS::get_singleton()->get_main_thread_id())) return error("WRONG_THREAD", "Document bridge requires the main thread.");
    return apply(document_.rename_mesh(utf8(id), utf8(name)));
}
kasane::Status KasaneDocumentBridge::create_view(const std::string &id) {
    const auto *mesh = document_.get_mesh(id);
    auto *view = memnew(KasaneMeshView);
    std::vector<uint32_t> dense;
    if (auto status = document_.render_indices(id, dense); !status.ok()) {
        memdelete(view);
        return status;
    }
    PackedInt32Array indices;
    for (auto slot : dense) indices.push_back(static_cast<int32_t>(slot));
    auto rendered = view->initialize(vectors(mesh->base_positions), vectors(mesh->uvs), indices, textures_.at(mesh->texture_asset_id));
    if (!bool(rendered["ok"])) {
        memdelete(view);
        return {utf8(rendered["code"]), utf8(rendered["message"])};
    }
    add_child(view);
    views_.emplace(id, view);
    return {};
}
Dictionary KasaneDocumentBridge::apply(const kasane::EditResult &edit) {
    auto out = result(edit.status);
    out["revision"] = edit.changes.revision;
    const auto kind = edit.changes.kind;
    out["change_kind"] = kind == kasane::ChangeKind::structure ? "structure" :
                         kind == kasane::ChangeKind::positions ? "positions" :
                         kind == kasane::ChangeKind::metadata ? "metadata" : "none";
    PackedStringArray changed;
    for (const auto &id : edit.changes.mesh_ids) changed.push_back(string(id));
    out["changed_meshes"] = changed;
    if (!edit.status.ok()) return out;
    // A render failure does not undo valid source data. Report it separately;
    // rebuilding the disposable preview is the recovery path.
    out["preview_ok"] = true;
    for (const auto &id : edit.changes.mesh_ids) {
        kasane::Status status;
        if (kind == kasane::ChangeKind::structure || !views_.contains(id)) status = create_view(id);
        else if (kind == kasane::ChangeKind::positions) {
            auto rendered = views_.at(id)->update_positions(vectors(document_.get_mesh(id)->base_positions));
            if (!bool(rendered["ok"])) status = {utf8(rendered["code"]), utf8(rendered["message"])};
        }
        if (!status.ok()) {
            out["preview_ok"] = false;
            out["preview_error"] = result(status);
        }
    }
    return out;
}
Dictionary KasaneDocumentBridge::get_mesh_snapshot(const String &id) const {
    if (!(OS::get_singleton()->get_thread_caller_id() == OS::get_singleton()->get_main_thread_id())) return error("WRONG_THREAD", "Document bridge requires the main thread.");
    const auto *mesh = document_.get_mesh(utf8(id));
    if (!mesh) return error("MISSING_MESH", "Mesh does not exist.");
    auto out = result({});
    out["id"] = string(mesh->id); out["name"] = string(mesh->name);
    out["texture_asset_id"] = string(mesh->texture_asset_id);
    out["vertex_ids"] = ids(mesh->vertex_ids);
    out["base_positions"] = vectors(mesh->base_positions); out["uvs"] = vectors(mesh->uvs);
    std::vector<uint32_t> triangles;
    for (const auto &triangle : mesh->triangles) triangles.insert(triangles.end(), triangle.begin(), triangle.end());
    out["triangles"] = ids(triangles);
    out["revision"] = document_.revision();
    return out;
}
Dictionary KasaneDocumentBridge::get_document_summary() const {
    if (!(OS::get_singleton()->get_thread_caller_id() == OS::get_singleton()->get_main_thread_id())) return error("WRONG_THREAD", "Document bridge requires the main thread.");
    Dictionary out;
    out["schema_version"] = kasane::Document::schema_version;
    out["initialized"] = document_.initialized(); out["id"] = string(document_.id());
    out["canvas_size"] = Vector2(document_.canvas().width, document_.canvas().height);
    out["revision"] = document_.revision(); out["asset_count"] = static_cast<int64_t>(document_.asset_count());
    Array meshes;
    for (const auto &id : document_.mesh_order()) {
        const auto *mesh = document_.get_mesh(id);
        Dictionary item;
        item["id"] = string(id); item["name"] = string(mesh->name);
        item["vertex_count"] = static_cast<int64_t>(mesh->vertex_ids.size());
        item["triangle_count"] = static_cast<int64_t>(mesh->triangles.size());
        meshes.push_back(item);
    }
    out["meshes"] = meshes;
    return out;
}
KasaneMeshView *KasaneDocumentBridge::get_mesh_view(const String &id) const {
    auto it = views_.find(utf8(id));
    return it == views_.end() ? nullptr : it->second;
}
Dictionary KasaneDocumentBridge::rebuild_preview() {
    if (!(OS::get_singleton()->get_thread_caller_id() == OS::get_singleton()->get_main_thread_id())) return error("WRONG_THREAD", "Document bridge requires the main thread.");
    for (const auto &[id, view] : views_) memdelete(view);
    views_.clear();
    for (const auto &id : document_.mesh_order())
        if (auto status = create_view(id); !status.ok()) return result(status);
    return result({});
}
}
