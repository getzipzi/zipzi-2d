// SPDX-License-Identifier: MIT
// SPDX-FileCopyrightText: 2023 MizunagiKB <mizukb@live.jp>
#ifndef INTERNAL_CUBISM_RENDERER_RESOURCE
#define INTERNAL_CUBISM_RENDERER_RESOURCE


// ----------------------------------------------------------------- include(s)
#include <gd_cubism.hpp>

#include <godot_cpp/classes/array_mesh.hpp>
#include <godot_cpp/classes/mesh_instance2d.hpp>
#include <godot_cpp/classes/node.hpp>
#include <godot_cpp/classes/shader.hpp>
#include <godot_cpp/classes/shader_material.hpp>
#include <godot_cpp/classes/sub_viewport.hpp>
#include <godot_cpp/classes/texture2d.hpp>
#include <godot_cpp/classes/image_texture.hpp>
#include <vector>

#include <gd_cubism_effect.hpp>


// ------------------------------------------------------------------ define(s)
// --------------------------------------------------------------- namespace(s)
using namespace Live2D::Cubism::Framework::Rendering;
using namespace godot;


// -------------------------------------------------------------------- enum(s)
// ------------------------------------------------------------------- const(s)
// ------------------------------------------------------------------ static(s)
// ----------------------------------------------------------- class:forward(s)
class GDCubismUserModel;


// ------------------------------------------------------------------- class(s)
class InternalCubismRendererResource {
public:
    InternalCubismRendererResource(GDCubismUserModel *owner_viewport);
    ~InternalCubismRendererResource();

    void clear();

    SubViewport* request_viewport();
    MeshInstance2D* request_mesh_instance();
    ShaderMaterial* request_shader_material(const Csm::CubismModel *model, const Csm::csmInt32 index);
    ShaderMaterial* request_mask_material();

    // Shader
    Ref<Shader> get_shader(const GDCubismShader e) const { return this->ary_shader[e]; }

public:
    GDCubismUserModel *_owner_viewport;

    TypedArray<Node> managed_nodes;
    Array ary_texture;
    Array ary_shader;
    Dictionary dict_mesh;
    Dictionary dict_mask;
    Dictionary dict_mask_meshes;
    SubViewport *mask_atlas = nullptr;
    Node2D *render_root = nullptr;
    Vector2i mask_cell_size;
    Ref<ImageTexture> position_texture;
    Ref<ImageTexture> parameter_texture;
    std::vector<int> vertex_offsets;
    int position_rows = 0;
    struct DrawableState {
        MeshInstance2D *node = nullptr;
        Ref<ArrayMesh> mesh;
        Ref<ShaderMaterial> material;
        Vector4 base, screen, multiply;
        Vector4 atlas_rect;
        Vector4 mask_transform;
        Vector4 pending_atlas_rect;
        Vector4 pending_mask_transform;
        int order = -2147483647;
        bool initialized = false;
        bool mask_initialized = false;
        bool mask_pending = false;
        bool visible = false;
    };
    std::vector<DrawableState> drawables;
    std::vector<MeshInstance2D *> batches;
    std::vector<int> batch_signature;
    bool batching = true;

    // Render parameters
    Vector2i vct_canvas_size;
    float CALCULATED_PPUNIT_C;
    Vector2 CALCULATED_ORIGIN_C;
};


// ------------------------------------------------------------------ method(s)


#endif // INTERNAL_CUBISM_RENDERER_RESOURCE
