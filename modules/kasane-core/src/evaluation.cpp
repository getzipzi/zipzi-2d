// SPDX-License-Identifier: MIT
#include <kasane/evaluation.hpp>
#include <PurismKeyform.h>
#include <algorithm>
#include <cmath>

namespace kasane {
Status to_runtime_positions(Canvas c,std::span<const Vec2> positions,std::vector<Vec2> &out) {
    std::vector<Vec2> result; result.reserve(positions.size());
    for(auto p:positions) {
        Vec2 q{float((double(p.x)-c.origin.x)/c.pixels_per_unit),float((double(c.origin.y)-p.y)/c.pixels_per_unit)};
        if(!std::isfinite(q.x)||!std::isfinite(q.y)) return Status::error("NON_FINITE","Position conversion overflows float32");
        result.push_back(q);
    }
    out=std::move(result); return {};
}
Status evaluate_frame(const Document &doc,const PreviewValues &preview,DrawableFrame &out) {
    if(!doc.initialized()) return Status::error("NOT_INITIALIZED","Initialize Document first");
    if(!doc.deformer_order().empty()) return Status::error("UNSUPPORTED_FEATURE",doc.deformer_order().front()+": legacy deformers are not supported by the formal evaluator");
    if(doc.mesh_order().size()>16777216 || doc.asset_order().size()>size_t(INT32_MAX)) return Status::error("CAPACITY","Object count exceeds runtime capacity");
    DrawableFrame frame; frame.canvas=doc.canvas(); frame.source_revision=doc.revision();
    for(const auto &[id,value]:preview) {
        if(!doc.get_parameter(id)) return Status::error("MISSING_PARAMETER",id);
        if(!std::isfinite(value)) return Status::error("NON_FINITE",id+".preview_value");
    }
    PreviewValues values;
    for(const auto &id:doc.parameter_order()) {
        const auto &p=*doc.get_parameter(id);
        const auto it=preview.find(id); const float requested=it==preview.end()?p.default_value:it->second;
        const float value=std::clamp(requested,p.minimum,p.maximum);
        frame.parameters.push_back({id,requested,value,requested!=value}); values[id]=value;
    }
    for(const auto &id:doc.mesh_order()) {
        if(!doc.parent_of(id).empty()||!doc.parent_of(id,true).empty()) return Status::error("UNSUPPORTED_FEATURE",id+": parent relations are not implemented in the formal evaluator");
        const auto &mesh=*doc.get_mesh(id);
        Drawable d; d.id=id; d.runtime_id=mesh.runtime_id; d.texture_asset_id=mesh.texture_asset_id;
        d.draw_order=d.render_order=int32_t(frame.drawables.size());
        auto slot=std::find(doc.asset_order().begin(),doc.asset_order().end(),mesh.texture_asset_id);
        if(slot==doc.asset_order().end()) return Status::error("MISSING_ASSET",id+".texture_asset_id");
        d.texture_slot=int32_t(slot-doc.asset_order().begin());
        if(auto s=to_runtime_positions(frame.canvas,mesh.base_positions,d.positions);!s.ok()) return Status::error(s.code,id+".base_positions: "+s.message);
        for(auto uv:mesh.uvs) d.uvs.push_back({uv.x,1-uv.y});
        if(auto s=doc.render_indices(id,d.indices);!s.ok()) return s;
        for(size_t i=0;i<d.indices.size();i+=3) std::swap(d.indices[i+1],d.indices[i+2]);
        if(const auto *binding=doc.binding_for_mesh(id)) {
            std::vector<psm__key_axis> axes;
            for(const auto &axis:binding->axes) {
                const auto &p=*doc.get_parameter(axis.parameter_id);
                const float epsilon=std::pow(0.1f,float(p.decimal_places));
                const auto segment=psm__find_key_segment(values.at(p.id),axis.keys.data(),int32_t(axis.keys.size()),epsilon,epsilon*1.5f);
                d.visible &= !segment.is_outside;
                axes.push_back({segment.index,int32_t(axis.keys.size()),segment.weight});
            }
            if(d.visible) {
                std::vector<int32_t> indices(size_t(1)<<axes.size());
                std::vector<float> weights(indices.size());
                auto count=psm__key_combinations(int32_t(axes.size()),axes.data(),indices.data(),weights.data());
                std::vector<std::vector<float>> targets(count);
                std::vector<float *> ptrs(count);
                for(size_t k=0;k<count;++k) {
                    std::vector<Vec2> converted;
                    if(auto s=to_runtime_positions(frame.canvas,binding->keyforms.at(indices[k]).positions,converted);!s.ok()) return Status::error(s.code,binding->id+".keyforms: "+s.message);
                    for(auto p:converted) { targets[k].push_back(p.x); targets[k].push_back(p.y); }
                    ptrs[k]=targets[k].data();
                }
                if(d.positions.size()>size_t(INT32_MAX)/2) return Status::error("CAPACITY",id+".positions");
                std::vector<float> xy(d.positions.size()*2);
                psm__blend_vectors(ptrs.data(),weights.data(),int32_t(count),int32_t(xy.size()),xy.data());
                for(size_t v=0;v<d.positions.size();++v) d.positions[v]={xy[2*v],xy[2*v+1]};
                if(auto s=validate_positions(d.positions);!s.ok()) return Status::error(s.code,id+".evaluated_positions");
            } else {
                // Disabled Core vertex buffers can contain a previous frame.
                // Consumers must use visibility, not stale geometry, to draw.
                std::fill(d.positions.begin(),d.positions.end(),Vec2{});
            }
        }
        frame.drawables.push_back(std::move(d));
    }
    out=std::move(frame); return {};
}
}
