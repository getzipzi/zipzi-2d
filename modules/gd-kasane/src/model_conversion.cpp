// SPDX-License-Identifier: MIT
#include "model_conversion.hpp"
#include <cmath>
using namespace godot;
namespace kasane_gd {
namespace {
bool numeric(const Variant &v) { return v.get_type()==Variant::INT || v.get_type()==Variant::FLOAT; }
kasane::Status fail() { return kasane::Status::error("INVALID_FIELD","Required field is absent or has the wrong type"); }
kasane::Status strings(const Dictionary &d,std::initializer_list<const char *> fields) {
    for(auto f:fields) if(!d.has(f)||d[f].get_type()!=Variant::STRING) return fail();
    return {};
}
kasane::Status floats(const Variant &v,std::vector<float> &out) {
    if(v.get_type()==Variant::PACKED_FLOAT32_ARRAY) {
        PackedFloat32Array a=v; for(int64_t i=0;i<a.size();++i) out.push_back(a[i]); return {};
    }
    if(v.get_type()!=Variant::ARRAY) return fail();
    Array a=v; for(int64_t i=0;i<a.size();++i) { if(!numeric(a[i])) return fail(); out.push_back(float(a[i])); }
    return {};
}
Array float_array(const std::vector<float> &v) { Array a; for(float x:v) a.push_back(x); return a; }
}
kasane::Status parameter_from_dictionary(const Dictionary &d,kasane::Parameter &p) {
    if(auto s=strings(d,{"id","runtime_id","name"});!s.ok()) return s;
    p.id=utf8(d["id"]); p.runtime_id=utf8(d["runtime_id"]); p.name=utf8(d["name"]);
    for(auto field:{"minimum","maximum","default_value"}) if(!d.has(field)||!numeric(d[field])) return fail();
    p.minimum=float(d["minimum"]); p.maximum=float(d["maximum"]); p.default_value=float(d["default_value"]);
    if(d.has("decimal_places")) {
        if(!numeric(d["decimal_places"])) return fail();
        double places=d["decimal_places"];
        if(!std::isfinite(places)||std::floor(places)!=places||places<0||places>9) return fail();
        p.decimal_places=int32_t(places);
    }
    return {};
}
kasane::Status binding_from_dictionary(const Dictionary &d,kasane::MeshBinding &b) {
    if(auto s=strings(d,{"id","mesh_id"});!s.ok()) return s;
    b.id=utf8(d["id"]); b.mesh_id=utf8(d["mesh_id"]);
    for(auto field:{"axes","keyforms"}) if(!d.has(field)||d[field].get_type()!=Variant::ARRAY) return fail();
    Array axes=d["axes"],forms=d["keyforms"];
    for(int64_t i=0;i<axes.size();++i) {
        if(axes[i].get_type()!=Variant::DICTIONARY) return fail();
        Dictionary a=axes[i]; if(auto s=strings(a,{"parameter_id"});!s.ok()) return s;
        if(!a.has("keys")) return fail();
        kasane::BindingAxis axis; axis.parameter_id=utf8(a["parameter_id"]);
        if(auto s=floats(a["keys"],axis.keys);!s.ok()) return s;
        b.axes.push_back(std::move(axis));
    }
    for(int64_t i=0;i<forms.size();++i) {
        if(forms[i].get_type()!=Variant::DICTIONARY) return fail();
        Dictionary f=forms[i]; if(!f.has("keys")||!f.has("positions")) return fail();
        kasane::MeshKeyform form;
        if(auto s=floats(f["keys"],form.keys);!s.ok()) return s;
        if(f["positions"].get_type()==Variant::PACKED_VECTOR2_ARRAY) form.positions=vectors(PackedVector2Array(f["positions"]));
        else if(f["positions"].get_type()==Variant::ARRAY) {
            Array points=f["positions"];
            for(int64_t j=0;j<points.size();++j) {
                if(points[j].get_type()!=Variant::ARRAY) return fail();
                Array p=points[j]; if(p.size()!=2||!numeric(p[0])||!numeric(p[1])) return fail();
                form.positions.push_back({float(p[0]),float(p[1])});
            }
        } else return fail();
        b.keyforms.push_back(std::move(form));
    }
    return {};
}
Dictionary parameter_dictionary(const kasane::Parameter &p) {
    Dictionary d; d["id"]=string(p.id); d["runtime_id"]=string(p.runtime_id); d["name"]=string(p.name);
    d["minimum"]=p.minimum; d["maximum"]=p.maximum; d["default_value"]=p.default_value; d["decimal_places"]=p.decimal_places; return d;
}
Dictionary binding_dictionary(const kasane::MeshBinding &b) {
    Dictionary d; d["id"]=string(b.id); d["mesh_id"]=string(b.mesh_id); Array axes,forms;
    for(const auto &a:b.axes) { Dictionary axis; axis["parameter_id"]=string(a.parameter_id); axis["keys"]=float_array(a.keys); axes.push_back(axis); }
    for(const auto &f:b.keyforms) {
        Dictionary form; form["keys"]=float_array(f.keys); Array points;
        for(auto p:f.positions) { Array point; point.push_back(p.x); point.push_back(p.y); points.push_back(point); }
        form["positions"]=points; forms.push_back(form);
    }
    d["axes"]=axes; d["keyforms"]=forms; return d;
}
}
