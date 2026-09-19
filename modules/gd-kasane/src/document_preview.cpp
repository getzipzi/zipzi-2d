// SPDX-License-Identifier: MIT
#include "document_preview.hpp"
#include <godot_cpp/core/class_db.hpp>
using namespace godot;
namespace kasane_gd {
void KasaneDocumentPreview::_bind_methods() {
    ClassDB::bind_method(D_METHOD("set_document","document"),&KasaneDocumentPreview::set_document);
    ClassDB::bind_method(D_METHOD("get_document"),&KasaneDocumentPreview::get_document);
    ClassDB::bind_method(D_METHOD("set_texture_store","textures"),&KasaneDocumentPreview::set_texture_store);
    ClassDB::bind_method(D_METHOD("refresh"),&KasaneDocumentPreview::refresh);
    ClassDB::bind_method(D_METHOD("get_last_result"),&KasaneDocumentPreview::get_last_result);
    ClassDB::bind_method(D_METHOD("get_mesh_view","mesh_id"),&KasaneDocumentPreview::get_mesh_view);
    ClassDB::bind_method(D_METHOD("_document_changed","change"),&KasaneDocumentPreview::document_changed);
}
void KasaneDocumentPreview::clear_views() {
    for(const auto &[id,view]:views_) memdelete(view);
    views_.clear();
}
void KasaneDocumentPreview::set_document(const Ref<KasaneDocumentBridge> &doc) {
    if(document_==doc) return;
    if(document_.is_valid()) {
        document_->disconnect("changed",Callable(this,"_document_changed"));
        document_->disconnect("preview_changed",Callable(this,"refresh"));
    }
    clear_views(); document_=doc;
    if(document_.is_valid()) {
        document_->connect("changed",Callable(this,"_document_changed"));
        document_->connect("preview_changed",Callable(this,"refresh"));
    }
    refresh();
}
void KasaneDocumentPreview::set_texture_store(const Ref<KasaneTextureStore> &textures) {
    if(textures_==textures) return;
    if(textures_.is_valid()) textures_->disconnect("changed",Callable(this,"refresh"));
    textures_=textures;
    if(textures_.is_valid()) textures_->connect("changed",Callable(this,"refresh"));
    refresh();
}
void KasaneDocumentPreview::document_changed(const Dictionary &) { refresh(); }
Dictionary KasaneDocumentPreview::refresh() {
    auto finish=[&](Dictionary result) { last_result_=result; return result; };
    if(document_.is_null()) { clear_views(); return finish(error("MISSING_DOCUMENT","Attach a Document.")); }
    kasane::DrawableFrame frame;
    if(auto s=document_->evaluate(frame);!s.ok()) return finish(result(s));
    if(textures_.is_null()) return finish(error("MISSING_TEXTURE_STORE","Attach a texture store."));
    std::unordered_map<std::string,KasaneMeshView *> pending;
    auto fail=[&](Dictionary error) { for(const auto &[id,view]:pending) memdelete(view); return finish(error); };
    for(const auto &d:frame.drawables) {
        auto texture=textures_->get_texture(string(d.texture_asset_id));
        const auto *asset=document_->source().get_asset(d.texture_asset_id);
        if(texture.is_null()) return fail(error("MISSING_TEXTURE","Preview texture is not loaded; source edits remain valid."));
        if(texture->get_width()!=int64_t(asset->width)||texture->get_height()!=int64_t(asset->height))
            return fail(error("RESOURCE_MISMATCH","Preview texture dimensions differ from source metadata."));
        std::vector<kasane::Vec2> positions,uvs;
        // Convert only at the Godot presentation boundary, back to canvas pixels.
        for(auto p:d.positions) positions.push_back({p.x*frame.canvas.pixels_per_unit+frame.canvas.origin.x,frame.canvas.origin.y-p.y*frame.canvas.pixels_per_unit});
        for(auto uv:d.uvs) uvs.push_back({uv.x,1-uv.y});
        PackedInt32Array indices; for(size_t i=0;i<d.indices.size();i+=3) {
            indices.push_back(d.indices[i]); indices.push_back(d.indices[i+2]); indices.push_back(d.indices[i+1]);
        }
        auto *view=memnew(KasaneMeshView); pending[d.id]=view;
        auto status=view->initialize(vectors(positions),vectors(uvs),indices,texture);
        if(!bool(status["ok"])) return fail(status);
        view->set_visible(d.visible); view->set_modulate(Color(1,1,1,d.opacity));
    }
    clear_views(); views_=std::move(pending);
    for(const auto &d:frame.drawables) add_child(views_.at(d.id));
    auto status=result({}); status["revision"]=frame.source_revision; return finish(status);
}
KasaneMeshView *KasaneDocumentPreview::get_mesh_view(const String &id) const {
    auto it=views_.find(utf8(id)); return it==views_.end()?nullptr:it->second;
}
}
