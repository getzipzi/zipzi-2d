// SPDX-License-Identifier: MIT
#include "document_bridge.hpp"
#include <godot_cpp/core/class_db.hpp>
#include <godot_cpp/classes/dir_access.hpp>
#include <godot_cpp/classes/file_access.hpp>
#include <godot_cpp/classes/json.hpp>
#include <godot_cpp/classes/os.hpp>
#include <godot_cpp/classes/project_settings.hpp>
#include <godot_cpp/classes/resource_loader.hpp>
#include <cmath>

using namespace godot;
namespace kasane_gd {
void KasaneDocumentBridge::_bind_methods() {
    ClassDB::bind_method(D_METHOD("initialize", "id", "canvas_size"), &KasaneDocumentBridge::initialize);
    ClassDB::bind_method(D_METHOD("add_image_asset", "id", "name", "source", "texture"), &KasaneDocumentBridge::add_image_asset);
    ClassDB::bind_method(D_METHOD("create_mesh", "description"), &KasaneDocumentBridge::create_mesh);
    ClassDB::bind_method(D_METHOD("set_vertex_positions", "mesh_id", "vertex_ids", "positions"), &KasaneDocumentBridge::set_vertex_positions);
    ClassDB::bind_method(D_METHOD("rename_mesh", "id", "name"), &KasaneDocumentBridge::rename_mesh);
    ClassDB::bind_method(D_METHOD("begin_transaction"), &KasaneDocumentBridge::begin_transaction);
    ClassDB::bind_method(D_METHOD("stage_vertex_positions", "mesh_id", "vertex_ids", "positions"), &KasaneDocumentBridge::stage_vertex_positions);
    ClassDB::bind_method(D_METHOD("commit_transaction"), &KasaneDocumentBridge::commit_transaction);
    ClassDB::bind_method(D_METHOD("cancel_transaction"), &KasaneDocumentBridge::cancel_transaction);
    ClassDB::bind_method(D_METHOD("get_mesh", "id"), &KasaneDocumentBridge::get_mesh);
    ClassDB::bind_method(D_METHOD("replace_mesh", "description"), &KasaneDocumentBridge::replace_mesh);
    ClassDB::bind_method(D_METHOD("capture_state"), &KasaneDocumentBridge::capture_state);
    ClassDB::bind_method(D_METHOD("restore_state", "state"), &KasaneDocumentBridge::restore_state);
    ClassDB::bind_method(D_METHOD("commit_vertex_updates", "updates", "expected_revision"), &KasaneDocumentBridge::commit_vertex_updates);
    ClassDB::bind_method(D_METHOD("get_asset_snapshot", "id"), &KasaneDocumentBridge::get_asset_snapshot);
    ClassDB::bind_method(D_METHOD("save_project", "path"), &KasaneDocumentBridge::save_project);
    ClassDB::bind_method(D_METHOD("open_project", "path"), &KasaneDocumentBridge::open_project);
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
Dictionary KasaneDocumentBridge::create_mesh(const Dictionary &d) { return write_mesh(d, false); }
Dictionary KasaneDocumentBridge::replace_mesh(const Dictionary &d) { return write_mesh(d, true); }
Dictionary KasaneDocumentBridge::write_mesh(const Dictionary &d, bool replace) {
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
    return apply(replace ? document_.replace_mesh(std::move(mesh)) : document_.create_mesh(std::move(mesh)));
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
Dictionary KasaneDocumentBridge::begin_transaction() {
    if (!(OS::get_singleton()->get_thread_caller_id() == OS::get_singleton()->get_main_thread_id())) return error("WRONG_THREAD", "Document bridge requires the main thread.");
    return result(document_.begin_transaction());
}
Dictionary KasaneDocumentBridge::stage_vertex_positions(const String &mesh_id, const PackedInt64Array &vertex_ids,
                                                        const PackedVector2Array &positions) {
    if (!(OS::get_singleton()->get_thread_caller_id() == OS::get_singleton()->get_main_thread_id())) return error("WRONG_THREAD", "Document bridge requires the main thread.");
    std::vector<uint32_t> vertices;
    if (auto status = ids(vertex_ids, vertices); !status.ok()) return result(status);
    return result(document_.stage_vertex_positions({utf8(mesh_id), std::move(vertices), vectors(positions)}));
}
Dictionary KasaneDocumentBridge::commit_transaction() {
    if (!(OS::get_singleton()->get_thread_caller_id() == OS::get_singleton()->get_main_thread_id())) return error("WRONG_THREAD", "Document bridge requires the main thread.");
    return apply(document_.commit_transaction());
}
Dictionary KasaneDocumentBridge::cancel_transaction() {
    if (!(OS::get_singleton()->get_thread_caller_id() == OS::get_singleton()->get_main_thread_id())) return error("WRONG_THREAD", "Document bridge requires the main thread.");
    return result(document_.cancel_transaction());
}
Dictionary KasaneDocumentBridge::commit_vertex_updates(const Array &updates, int64_t expected_revision) {
    if (!(OS::get_singleton()->get_thread_caller_id() == OS::get_singleton()->get_main_thread_id())) return error("WRONG_THREAD", "Document bridge requires the main thread.");
    if (expected_revision < 0) return error("INVALID_REVISION", "Expected revision must be nonnegative.");
    std::vector<kasane::VertexPositionUpdate> batch;
    batch.reserve(updates.size());
    for (int64_t i = 0; i < updates.size(); ++i) {
        if (updates[i].get_type() != Variant::DICTIONARY) return error("INVALID_FIELD", "Each update must be a Dictionary.");
        Dictionary item = updates[i];
        if (!item.has("mesh_id") || item["mesh_id"].get_type() != Variant::STRING ||
            !item.has("vertex_ids") || item["vertex_ids"].get_type() != Variant::PACKED_INT64_ARRAY ||
            !item.has("positions") || item["positions"].get_type() != Variant::PACKED_VECTOR2_ARRAY)
            return error("INVALID_FIELD", "Update requires mesh_id, vertex_ids and positions.");
        kasane::VertexPositionUpdate update;
        update.mesh_id = utf8(item["mesh_id"]);
        if (auto status = ids(item["vertex_ids"], update.vertex_ids); !status.ok()) return result(status);
        update.positions = vectors(PackedVector2Array(item["positions"]));
        batch.push_back(std::move(update));
    }
    return apply(document_.apply_vertex_position_updates_at_revision(batch, static_cast<uint64_t>(expected_revision)));
}
Dictionary KasaneDocumentBridge::get_asset_snapshot(const String &id) const {
    if (!(OS::get_singleton()->get_thread_caller_id() == OS::get_singleton()->get_main_thread_id())) return error("WRONG_THREAD", "Document bridge requires the main thread.");
    const auto *asset = document_.get_asset(utf8(id));
    if (!asset) return error("MISSING_ASSET", "Asset does not exist.");
    auto out = result({});
    out["id"] = string(asset->id); out["name"] = string(asset->name); out["source"] = string(asset->source);
    out["width"] = static_cast<int64_t>(asset->width); out["height"] = static_cast<int64_t>(asset->height);
    out["revision"] = document_.revision();
    return out;
}

namespace {
constexpr int64_t PROJECT_FORMAT_VERSION = 1;
Dictionary project_dictionary(const kasane::Document &document) {
    Dictionary root;
    root["format"] = "kasane-project";
    root["format_version"] = PROJECT_FORMAT_VERSION;
    Dictionary doc;
    doc["id"] = string(document.id());
    Array canvas;
    canvas.push_back(document.canvas().width); canvas.push_back(document.canvas().height);
    doc["canvas"] = canvas;
    Array assets;
    for (const auto &id : document.asset_order()) {
        const auto *asset = document.get_asset(id);
        Dictionary item;
        item["id"] = string(asset->id); item["name"] = string(asset->name); item["source"] = string(asset->source);
        item["width"] = static_cast<int64_t>(asset->width); item["height"] = static_cast<int64_t>(asset->height);
        assets.push_back(item);
    }
    doc["assets"] = assets;
    Array meshes;
    for (const auto &id : document.mesh_order()) {
        const auto *mesh = document.get_mesh(id);
        Dictionary item;
        item["id"] = string(mesh->id); item["name"] = string(mesh->name); item["texture_asset_id"] = string(mesh->texture_asset_id);
        item["vertex_ids"] = ids(mesh->vertex_ids);
        Array positions;
        for (const auto &p : mesh->base_positions) { Array pair; pair.push_back(p.x); pair.push_back(p.y); positions.push_back(pair); }
        item["base_positions"] = positions;
        Array uvs;
        for (const auto &p : mesh->uvs) { Array pair; pair.push_back(p.x); pair.push_back(p.y); uvs.push_back(pair); }
        item["uvs"] = uvs;
        std::vector<uint32_t> triangles;
        for (const auto &triangle : mesh->triangles) triangles.insert(triangles.end(), triangle.begin(), triangle.end());
        item["triangles"] = ids(triangles);
        meshes.push_back(item);
    }
    doc["meshes"] = meshes;
    root["document"] = doc;
    return root;
}
bool number(const Variant &value) { return value.get_type() == Variant::INT || value.get_type() == Variant::FLOAT; }
bool unsigned_integer(const Variant &value, uint64_t maximum, uint64_t &out) {
    if (!number(value)) return false;
    const double parsed = static_cast<double>(value);
    if (!std::isfinite(parsed) || parsed < 0 || parsed > static_cast<double>(maximum) || std::floor(parsed) != parsed) return false;
    out = static_cast<uint64_t>(parsed);
    return true;
}
kasane::Status require(const Dictionary &d, const char *key, Variant::Type type) {
    if (!d.has(key) || d[key].get_type() != type) return kasane::Status::error("INVALID_PROJECT", utf8(String("Missing or invalid field: ") + String(key)));
    return {};
}
kasane::Status parse_vectors(const Variant &value, std::vector<kasane::Vec2> &out) {
    if (value.get_type() != Variant::ARRAY) return kasane::Status::error("INVALID_PROJECT", "Expected an array of coordinate pairs.");
    Array array = value;
    for (int64_t i = 0; i < array.size(); ++i) {
        if (array[i].get_type() != Variant::ARRAY) return kasane::Status::error("INVALID_PROJECT", "Expected a coordinate pair.");
        Array pair = array[i];
        if (pair.size() != 2 || !number(pair[0]) || !number(pair[1])) return kasane::Status::error("INVALID_PROJECT", "Coordinates require two numbers.");
        out.push_back({static_cast<float>(pair[0]), static_cast<float>(pair[1])});
    }
    return {};
}
kasane::Status parse_vertex_ids(const Variant &value, std::vector<uint32_t> &out) {
    if (value.get_type() != Variant::ARRAY) return kasane::Status::error("INVALID_PROJECT", "Expected an array of vertex IDs.");
    Array array = value;
    for (int64_t i = 0; i < array.size(); ++i) {
        uint64_t parsed = 0;
        if (!unsigned_integer(array[i], UINT32_MAX, parsed))
            return kasane::Status::error("INVALID_VERTEX_ID", "Vertex IDs must fit uint32.");
        out.push_back(static_cast<uint32_t>(parsed));
    }
    return {};
}
kasane::Status parse_project(const Dictionary &root, kasane::Document &document,
                             std::unordered_map<std::string, Ref<Texture2D>> &textures) {
    if (auto s = require(root, "format", Variant::STRING); !s.ok()) return s;
    if (String(root["format"]) != "kasane-project") return kasane::Status::error("INVALID_PROJECT", "Not a Kasane project file.");
    uint64_t format_version = 0;
    if (!root.has("format_version") || !unsigned_integer(root["format_version"], UINT32_MAX, format_version))
        return kasane::Status::error("INVALID_PROJECT", "Missing integer format_version.");
    if (format_version != PROJECT_FORMAT_VERSION)
        return kasane::Status::error("UNSUPPORTED_VERSION", "This project format version is not supported.");
    if (auto s = require(root, "document", Variant::DICTIONARY); !s.ok()) return s;
    Dictionary doc = root["document"];
    if (auto s = require(doc, "id", Variant::STRING); !s.ok()) return s;
    if (auto s = require(doc, "canvas", Variant::ARRAY); !s.ok()) return s;
    Array canvas = doc["canvas"];
    if (canvas.size() != 2 || !number(canvas[0]) || !number(canvas[1])) return kasane::Status::error("INVALID_PROJECT", "Canvas requires two numbers.");
    if (auto s = document.initialize(utf8(doc["id"]), {static_cast<float>(canvas[0]), static_cast<float>(canvas[1])}); !s.ok()) return s;
    if (auto s = require(doc, "assets", Variant::ARRAY); !s.ok()) return s;
    Array assets = doc["assets"];
    for (int64_t i = 0; i < assets.size(); ++i) {
        if (assets[i].get_type() != Variant::DICTIONARY) return kasane::Status::error("INVALID_PROJECT", "Asset entries must be objects.");
        Dictionary item = assets[i];
        for (const auto key : {"id", "name", "source"}) if (auto s = require(item, key, Variant::STRING); !s.ok()) return s;
        uint64_t width_value = 0, height_value = 0;
        if (!item.has("width") || !unsigned_integer(item["width"], UINT32_MAX, width_value) ||
            !item.has("height") || !unsigned_integer(item["height"], UINT32_MAX, height_value))
            return kasane::Status::error("INVALID_PROJECT", "Asset dimensions must be integers.");
        const auto width = static_cast<int64_t>(width_value), height = static_cast<int64_t>(height_value);
        if (width <= 0 || height <= 0)
            return kasane::Status::error("INVALID_ASSET", "Asset dimensions are outside uint32.");
        const String source = item["source"];
        if (!ResourceLoader::get_singleton()->exists(source, "Texture2D"))
            return kasane::Status::error("MISSING_RESOURCE", "Texture resource could not be loaded: " + utf8(source));
        Ref<Resource> resource = ResourceLoader::get_singleton()->load(source);
        Ref<Texture2D> texture = resource;
        if (texture.is_null()) return kasane::Status::error("MISSING_RESOURCE", "Texture resource could not be loaded: " + utf8(source));
        if (texture->get_width() != width || texture->get_height() != height)
            return kasane::Status::error("RESOURCE_MISMATCH", "Texture dimensions no longer match the saved project: " + utf8(source));
        const auto id = utf8(item["id"]);
        auto edit = document.add_asset({id, utf8(item["name"]), utf8(source), static_cast<uint32_t>(width), static_cast<uint32_t>(height)});
        if (!edit.status.ok()) return edit.status;
        textures.emplace(id, texture);
    }
    if (auto s = require(doc, "meshes", Variant::ARRAY); !s.ok()) return s;
    Array meshes = doc["meshes"];
    for (int64_t i = 0; i < meshes.size(); ++i) {
        if (meshes[i].get_type() != Variant::DICTIONARY) return kasane::Status::error("INVALID_PROJECT", "Mesh entries must be objects.");
        Dictionary item = meshes[i];
        for (const auto key : {"id", "name", "texture_asset_id"}) if (auto s = require(item, key, Variant::STRING); !s.ok()) return s;
        kasane::Mesh mesh;
        mesh.id = utf8(item["id"]); mesh.name = utf8(item["name"]); mesh.texture_asset_id = utf8(item["texture_asset_id"]);
        if (!item.has("vertex_ids") || !item.has("base_positions") || !item.has("uvs") || !item.has("triangles"))
            return kasane::Status::error("INVALID_PROJECT", "Mesh geometry fields are required.");
        if (auto s = parse_vertex_ids(item["vertex_ids"], mesh.vertex_ids); !s.ok()) return s;
        if (auto s = parse_vectors(item["base_positions"], mesh.base_positions); !s.ok()) return s;
        if (auto s = parse_vectors(item["uvs"], mesh.uvs); !s.ok()) return s;
        std::vector<uint32_t> flat;
        if (auto s = parse_vertex_ids(item["triangles"], flat); !s.ok()) return s;
        if (flat.size() % 3) return kasane::Status::error("INVALID_LENGTH", "Triangle vertex IDs must be a multiple of three.");
        for (size_t j = 0; j < flat.size(); j += 3) mesh.triangles.push_back({flat[j], flat[j + 1], flat[j + 2]});
        auto edit = document.create_mesh(std::move(mesh));
        if (!edit.status.ok()) return edit.status;
    }
    document.mark_saved();
    return {};
}
}

Dictionary KasaneDocumentBridge::save_project(const String &path) {
    if (!(OS::get_singleton()->get_thread_caller_id() == OS::get_singleton()->get_main_thread_id())) return error("WRONG_THREAD", "Document bridge requires the main thread.");
    if (!document_.initialized()) return error("NOT_INITIALIZED", "Initialize Document first.");
    if (document_.transaction_active()) return error("TRANSACTION_ACTIVE", "Commit or cancel the active transaction before saving.");
    const String absolute = ProjectSettings::get_singleton()->globalize_path(path);
    const String temporary = absolute + String(".tmp.") + String::num_int64(OS::get_singleton()->get_process_id());
    Ref<FileAccess> file = FileAccess::open(temporary, FileAccess::WRITE);
    if (file.is_null()) return error("SAVE_FAILED", "Could not create a temporary project file.");
    file->store_string(JSON::stringify(project_dictionary(document_), "  ", true, true) + "\n");
    file->flush();
    const Error write_error = file->get_error();
    file.unref();
    if (write_error != OK) {
        DirAccess::remove_absolute(temporary);
        return error("SAVE_FAILED", "Could not finish writing the temporary project file.");
    }
    const Error rename_error = DirAccess::rename_absolute(temporary, absolute);
    if (rename_error != OK) {
        DirAccess::remove_absolute(temporary);
        return error("SAVE_FAILED", "Could not atomically replace the project file; the previous file was kept.");
    }
    document_.mark_saved();
    saved_content_ = content_signature();
    auto out = result({});
    out["path"] = path;
    return out;
}
Dictionary KasaneDocumentBridge::open_project(const String &path) {
    if (!(OS::get_singleton()->get_thread_caller_id() == OS::get_singleton()->get_main_thread_id())) return error("WRONG_THREAD", "Document bridge requires the main thread.");
    const String absolute = ProjectSettings::get_singleton()->globalize_path(path);
    Ref<FileAccess> file = FileAccess::open(absolute, FileAccess::READ);
    if (file.is_null()) return error("OPEN_FAILED", "Project file could not be opened.");
    Ref<JSON> json;
    json.instantiate();
    const Error parse_error = json->parse(file->get_as_text());
    if (parse_error != OK || json->get_data().get_type() != Variant::DICTIONARY) return error("INVALID_PROJECT", "Project file is not valid JSON object data.");
    kasane::Document candidate;
    std::unordered_map<std::string, Ref<Texture2D>> candidate_textures;
    if (auto status = parse_project(json->get_data(), candidate, candidate_textures); !status.ok()) return result(status);
    for (const auto &[id, view] : views_) memdelete(view);
    views_.clear();
    document_.restore_from(candidate);
    ++generation_;
    saved_content_ = content_signature();
    textures_ = std::move(candidate_textures);
    auto rebuilt = rebuild_preview();
    if (!bool(rebuilt["ok"])) return rebuilt;
    auto out = result({});
    out["path"] = path;
    out["revision"] = document_.revision();
    return out;
}
String KasaneDocumentBridge::content_signature() const {
    return JSON::stringify(project_dictionary(document_), "", true, true);
}
Ref<KasaneMeshData> KasaneDocumentBridge::get_mesh(const String &id) const {
    if (OS::get_singleton()->get_thread_caller_id() != OS::get_singleton()->get_main_thread_id()) return {};
    if (!document_.get_mesh(utf8(id))) return {};
    Ref<KasaneMeshData> handle;
    handle.instantiate();
    handle->attach(get_instance_id(), generation_, id);
    return handle;
}
Ref<KasaneDocumentState> KasaneDocumentBridge::capture_state() const {
    if (OS::get_singleton()->get_thread_caller_id() != OS::get_singleton()->get_main_thread_id()) return {};
    if (document_.transaction_active()) return {};
    Ref<KasaneDocumentState> state;
    state.instantiate();
    state->document = document_; state->textures = textures_;
    state->owner = get_instance_id(); state->generation = generation_;
    return state;
}
Dictionary KasaneDocumentBridge::restore_state(const Ref<KasaneDocumentState> &state) {
    if (OS::get_singleton()->get_thread_caller_id() != OS::get_singleton()->get_main_thread_id()) return error("WRONG_THREAD", "Document bridge requires the main thread.");
    if (state.is_null() || state->owner != get_instance_id() || state->generation != generation_)
        return error("STALE_STATE", "State belongs to another document session.");
    if (document_.transaction_active()) return error("TRANSACTION_ACTIVE", "Commit or cancel the transaction first.");
    document_.restore_from(state->document);
    textures_ = state->textures;
    return rebuild_preview();
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
        if (kind == kasane::ChangeKind::structure || !views_.contains(id)) {
            if (views_.contains(id)) { memdelete(views_.at(id)); views_.erase(id); }
            status = create_view(id);
        }
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
    out["modified"] = content_signature() != saved_content_; out["transaction_active"] = document_.transaction_active();
    out["generation"] = generation_;
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
