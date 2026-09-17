// SPDX-License-Identifier: MIT
// SPDX-FileCopyrightText: 2023 MizunagiKB <mizukb@live.jp>
// ----------------------------------------------------------------- include(s)
#include <gd_cubism.hpp>
#ifdef GD_CUBISM_USE_RENDERER_2D

#include <godot_cpp/classes/shader_material.hpp>
#include <godot_cpp/classes/viewport_texture.hpp>
#include <godot_cpp/classes/rendering_server.hpp>
#include <godot_cpp/variant/utility_functions.hpp>

#include <CubismFramework.hpp>
#include <Model/CubismModel.hpp>
#include <Rendering/CubismRenderer.hpp>

#include <private/internal_cubism_renderer_2d.hpp>
#include <private/internal_cubism_renderer_resource.hpp>
#include <private/internal_cubism_user_model.hpp>
#include <cfloat>
#include <vector>
#include <algorithm>
#include <godot_cpp/classes/image.hpp>
#include <godot_cpp/classes/project_settings.hpp>

// ------------------------------------------------------------------ define(s)
// --------------------------------------------------------------- namespace(s)
using namespace Live2D::Cubism::Core;
using namespace Live2D::Cubism::Framework;
using namespace Live2D::Cubism::Framework::Rendering;
using namespace godot;

// -------------------------------------------------------------------- enum(s)
// ------------------------------------------------------------------- const(s)
// ------------------------------------------------------------------ static(s)
PackedInt32Array make_Indices(const csmUint16 *ptr, const int32_t &size);
PackedVector2Array make_UVs(const Live2D::Cubism::Core::csmVector2 *ptr, const int32_t &size);
PackedVector2Array make_Vertices(const Live2D::Cubism::Core::csmVector2 *ptr, const int32_t &size, const Csm::csmFloat32 &ppunit);
const Vector4 make_vector4(const Live2D::Cubism::Core::csmVector4 &src_vec4);

// ----------------------------------------------------------- class:forward(s)
// ------------------------------------------------------------------- class(s)
InternalCubismRenderer2D::InternalCubismRenderer2D(Csm::csmUint32 width, Csm::csmUint32 height)
    : CubismRenderer(width, height)
{
}

InternalCubismRenderer2D::~InternalCubismRenderer2D()
{
}

void InternalCubismRenderer2D::update_material(const Csm::CubismModel *model, const Csm::csmInt32 index, const Ref<ShaderMaterial> mat) const
{
    const CubismTextureColor color_base = this->GetModelColorWithOpacity(model->GetDrawableOpacity(index));

    mat->set_shader_parameter("color_base", Vector4(color_base.R, color_base.G, color_base.B, color_base.A));
    mat->set_shader_parameter("color_screen", make_vector4(model->GetDrawableScreenColor(index)));
    mat->set_shader_parameter("color_multiply", make_vector4(model->GetDrawableMultiplyColor(index)));
}

void InternalCubismRenderer2D::make_ArrayMesh_prepare(
    const Csm::CubismModel *model,
    InternalCubismRendererResource &res)
{
    const Vector2 vct_size = this->get_size(model);
    const Vector2 vct_origin = this->get_origin(model);
    const float ppunit = this->get_ppunit(model);

    res.vct_canvas_size = vct_size;
    res.CALCULATED_ORIGIN_C = vct_origin;
    res.CALCULATED_PPUNIT_C = ppunit;
}

void InternalCubismRenderer2D::update_mesh(
    const Csm::CubismModel *model,
    const Csm::csmInt32 index,
    const InternalCubismRendererResource &res,
    const Ref<ArrayMesh> ary_mesh
) const
{
    auto pp_unit = res.CALCULATED_PPUNIT_C;

    if (ary_mesh->get_surface_count() > 0) {
        const int size = model->GetDrawableVertexCount(index);
        const auto ptr = model->GetDrawableVertexPositions(index);
        PackedByteArray upload;
        float *vertices = nullptr;
        if (res.position_texture.is_null()) {
            upload.resize(size * 2 * sizeof(float));
            vertices = reinterpret_cast<float *>(upload.ptrw());
        }


        Vector3 vct_min(DBL_MAX, DBL_MAX, 0.0);
        Vector3 vct_max(-DBL_MAX, -DBL_MAX, 0.0);

        for (int i = 0; i < size; i++)
        {
            float x = ptr[i].X * pp_unit;
            float y = ptr[i].Y * pp_unit;
            vct_min.x = Math::min(vct_min.x, x); // left
            vct_min.y = Math::min(vct_min.y, y); // top
            vct_max.x = Math::max(vct_max.x, x); // right
            vct_max.y = Math::max(vct_max.y, y); // bottom
            if (vertices) { vertices[i * 2] = x; vertices[i * 2 + 1] = -y; }
            
        }

        // Positions are uploaded once per model through position_texture.
        if (vertices) ary_mesh->surface_update_vertex_region(0, 0, upload);

        // aabb does not get automatically updated when directly updating the vertex region
        AABB aabb(Vector3(vct_min.x, -vct_max.y, 0), vct_max - vct_min);
        ary_mesh->set_custom_aabb(aabb);

        return;
    }

    Array ary;

    ary.resize(Mesh::ARRAY_MAX);

    PackedVector2Array vertex_ids;
    vertex_ids.resize(model->GetDrawableVertexCount(index));
    Vector2 *ids = vertex_ids.ptrw();
    for (int i = 0; i < vertex_ids.size(); ++i)
        ids[i] = Vector2(res.vertex_offsets[index] + i, 0);
    ary[Mesh::ARRAY_VERTEX] = vertex_ids;
    if (res.position_texture.is_null())
        ary[Mesh::ARRAY_VERTEX] = make_Vertices(model->GetDrawableVertexPositions(index), model->GetDrawableVertexCount(index), pp_unit);

    ary[Mesh::ARRAY_TEX_UV] = make_UVs(
        model->GetDrawableVertexUvs(index),
        model->GetDrawableVertexCount(index));

    ary[Mesh::ARRAY_INDEX] = make_Indices(
        model->GetDrawableVertexIndices(index),
        model->GetDrawableVertexIndexCount(index));

    ary_mesh->add_surface_from_arrays(Mesh::PRIMITIVE_TRIANGLES, ary);
    ary_mesh->set_custom_aabb(ary_mesh->get_aabb());
    update_mesh(model, index, res, ary_mesh);
}

Vector2 InternalCubismRenderer2D::get_size(const Csm::CubismModel *model) const
{
    Live2D::Cubism::Core::csmVector2 vct_size;
    Live2D::Cubism::Core::csmVector2 vct_origin;
    Csm::csmFloat32 ppunit;

    Live2D::Cubism::Core::csmReadCanvasInfo(model->GetModel(), &vct_size, &vct_origin, &ppunit);

    return Vector2(vct_size.X, vct_size.Y);
}

Vector2 InternalCubismRenderer2D::get_origin(const Csm::CubismModel *model) const
{
    Live2D::Cubism::Core::csmVector2 vct_size;
    Live2D::Cubism::Core::csmVector2 vct_origin;
    Csm::csmFloat32 ppunit;

    Live2D::Cubism::Core::csmReadCanvasInfo(model->GetModel(), &vct_size, &vct_origin, &ppunit);

    return Vector2(vct_origin.X, vct_origin.Y);
}

float InternalCubismRenderer2D::get_ppunit(const Csm::CubismModel *model) const
{
    Live2D::Cubism::Core::csmVector2 vct_size;
    Live2D::Cubism::Core::csmVector2 vct_origin;
    Csm::csmFloat32 ppunit;

    Live2D::Cubism::Core::csmReadCanvasInfo(model->GetModel(), &vct_size, &vct_origin, &ppunit);

    return ppunit;
}

void InternalCubismRenderer2D::update(InternalCubismRendererResource &res, int32_t mask_viewport_size)
{
    const CubismModel *model = this->GetModel();
    const uint32_t visibility_layer = res._owner_viewport->get_visibility_layer();
    if (res.render_root && res.render_root->get_visibility_layer() != visibility_layer)
        res.render_root->set_visibility_layer(visibility_layer);
    for (auto &drawable : res.drawables)
        if (drawable.node && drawable.node->get_visibility_layer() != visibility_layer)
            drawable.node->set_visibility_layer(visibility_layer);
    for (auto *batch : res.batches)
        if (batch->get_visibility_layer() != visibility_layer)
            batch->set_visibility_layer(visibility_layer);
    if (res.position_texture.is_valid()) {
        PackedByteArray data;
        data.resize(1024 * res.position_rows * 4 * sizeof(float));
        float *dst = reinterpret_cast<float *>(data.ptrw());
        for (int index = 0; index < model->GetDrawableCount(); ++index) {
            const auto *positions = model->GetDrawableVertexPositions(index);
            for (int v = 0; v < model->GetDrawableVertexCount(index); ++v) {
                const int offset = (res.vertex_offsets[index] + v) * 4;
                dst[offset] = positions[v].X * res.CALCULATED_PPUNIT_C;
                dst[offset + 1] = -positions[v].Y * res.CALCULATED_PPUNIT_C;
                dst[offset + 2] = index;
                dst[offset + 3] = 1;
            }
        }
        res.position_texture->update(Image::create_from_data(1024, res.position_rows, false, Image::FORMAT_RGBAF, data));
    }
    const Csm::csmInt32 *renderOrder = model->GetRenderOrders();
    const Csm::csmInt32 *maskCount = model->GetDrawableMaskCounts();

    this->make_ArrayMesh_prepare(
        model,
        res);

    // get the model's global transform to preform optimizations against
    auto mesh_0 = Object::cast_to<MeshInstance2D>(res.dict_mesh.values()[0]);

    const Transform2D viewport_transform = mesh_0->get_global_transform_with_canvas();
    const Rect2 viewport_bounds = mesh_0->get_viewport_rect();

    // update meshes
    for (Csm::csmInt32 index = 0; index < model->GetDrawableCount(); index++)
    {
        if (model->GetDrawableVertexCount(index) == 0)
            continue;
        if (model->GetDrawableVertexIndexCount(index) == 0)
            continue;
        
        auto &state = res.drawables[index];
        MeshInstance2D *node = state.node;
        if (node == nullptr) {
            continue;
        }
        const bool visible = model->GetDrawableDynamicFlagIsVisible(index) && model->GetDrawableOpacity(index) > 0.0f;
        if (!state.initialized || (!res.batching && visible != state.visible))
            node->set_visible(res.batching ? false : visible);
        state.visible = visible;
        Ref<ShaderMaterial> mat = state.material;
        Ref<ArrayMesh> ary_mesh = state.mesh;

        if (model->GetDrawableDynamicFlagVertexPositionsDidChange(index))
            this->update_mesh(model, index, res, ary_mesh);
        const auto color = GetModelColorWithOpacity(model->GetDrawableOpacity(index));
        const Vector4 base(color.R, color.G, color.B, visible ? color.A : 0.0f);
        const Vector4 screen = make_vector4(model->GetDrawableScreenColor(index));
        const Vector4 multiply = make_vector4(model->GetDrawableMultiplyColor(index));
        // Drawable order is local to the model, not Godot's canvas-wide z-index.
        state.base = base; state.screen = screen; state.multiply = multiply;
        state.order = renderOrder[index];
        state.initialized = true;
        
        // adjust real bounds to prevent the mesh being culled
        AABB bounds = ary_mesh->get_custom_aabb();
        Rect2 canvas_bounds = Rect2(bounds.position.x, bounds.position.y, bounds.size.x, bounds.size.y);
        if (!res.batching) RenderingServer::get_singleton()->canvas_item_set_custom_rect(
            node->get_canvas_item(), true,
            canvas_bounds
        );
    }

    // update masks
    Array masks = res.dict_mask.keys();
    const int columns = Math::max(1, int(Math::ceil(Math::sqrt(double(masks.size())))));
    const int rows = (masks.size() + columns - 1) / columns;
    std::vector<Rect2> mask_bounds;
    std::vector<double> mask_scales;
    bool any_visible = false;
    Vector2i required_cell(32, 32);
    for (int i = 0; i < masks.size(); i++) {
        String mask_name = masks[i];
        Node2D *mask = Object::cast_to<Node2D>(res.dict_mask[mask_name]);

        // build bounding box of all the meshes in the viewport
        AABB aabb;
        {
            Ref<ArrayMesh> mesh = Object::cast_to<MeshInstance2D>(mask->get_child(0))->get_mesh();
            aabb = mesh->get_custom_aabb();

            for (int n = 1; n < mask->get_child_count(); n++) {
                Ref<ArrayMesh> mesh = Object::cast_to<MeshInstance2D>(mask->get_child(n))->get_mesh();
                aabb = aabb.merge(mesh->get_custom_aabb());
            }
        }
        aabb = aabb.grow(4.0);  // adds padding around the mask for safety
        Rect2 bounds(aabb.position.x, aabb.position.y, aabb.size.x, aabb.size.y);

        // detect if the canvas item is going to be culled
        // only cull viewports when not looking at the model in the editor
        Rect2 bounds_in_viewport = viewport_transform.xform(bounds);
        const bool is_culled = 
            !Engine::get_singleton()->is_editor_hint() &&
            !(
                viewport_bounds.intersects(bounds_in_viewport) 
                || viewport_bounds.encloses(bounds_in_viewport)
            );

        mask->set_visible(!is_culled);
        any_visible |= !is_culled;

        // Allocate in screen pixels, not the model's authoring resolution.
        // Basis lengths also account for rotated, mirrored and canvas-scaled models.
        double scalar = Math::max(viewport_transform[0].length(), viewport_transform[1].length());
        scalar = Math::max(scalar, 0.0001);
        if (mask_viewport_size > 0) {
            scalar = Math::min(scalar, double(mask_viewport_size) / Math::max(1.0, double(Math::max(bounds.size.x, bounds.size.y))));
        }
        // Bound the atlas to 4096 pixels per axis even for extreme zoom.
        const int cell_limit = Math::max(32, (4096 / columns - 4) / 32 * 32);
        scalar = Math::min(scalar, double(cell_limit) / Math::max(1.0, double(Math::max(bounds.size.x, bounds.size.y))));
        // Quantized high-water allocations avoid rebuilding render targets as
        // animated bounds fluctuate. UVs use the actual texture dimensions.
        Vector2i mask_size(
            Math::max(32, int(Math::ceil(bounds.size.x * scalar / 32.0)) * 32),
            Math::max(32, int(Math::ceil(bounds.size.y * scalar / 32.0)) * 32));
        required_cell.x = Math::max(required_cell.x, mask_size.x + 4);
        required_cell.y = Math::max(required_cell.y, mask_size.y + 4);
        mask_bounds.push_back(bounds);
        mask_scales.push_back(scalar);
    }
    if (res.mask_atlas) {
    res.mask_atlas->set_update_mode(any_visible ? SubViewport::UPDATE_ALWAYS : SubViewport::UPDATE_DISABLED);
    // Shrink only after a substantial zoom-out, avoiding animation-sized churn.
    res.mask_cell_size.x = required_cell.x * 2 < res.mask_cell_size.x ? required_cell.x : Math::max(res.mask_cell_size.x, required_cell.x);
    res.mask_cell_size.y = required_cell.y * 2 < res.mask_cell_size.y ? required_cell.y : Math::max(res.mask_cell_size.y, required_cell.y);
    const Vector2i atlas_size(res.mask_cell_size.x * columns, res.mask_cell_size.y * rows);
    if (res.mask_atlas->get_size() != atlas_size) res.mask_atlas->set_size(atlas_size);
    for (int i = 0; i < masks.size(); ++i) {
        String mask_name = masks[i];
        Node2D *mask = Object::cast_to<Node2D>(res.dict_mask[mask_name]);
        const Rect2 bounds = mask_bounds[i];
        const double scalar = mask_scales[i];
        const Vector2 atlas_offset((i % columns) * res.mask_cell_size.x + 2, (i / columns) * res.mask_cell_size.y + 2);

        Vector2 viewport_offset = bounds.position;
        Transform2D transform = Transform2D(0, -viewport_offset);
        transform.scale(Vector2(scalar, scalar));
        transform[2] += atlas_offset;
        mask->set_transform(transform);

        Array meshes = res.dict_mask_meshes[mask_name];
        for (int n = 0; n < meshes.size(); n++) {
            MeshInstance2D *mesh = Object::cast_to<MeshInstance2D>(meshes[n]);
            Ref<ShaderMaterial> mat = mesh->get_material();
            auto &state = res.drawables[int(mesh->get_meta("drawable_index"))];
            const Vector4 next_atlas_rect(atlas_offset.x, atlas_offset.y, bounds.size.x * scalar, bounds.size.y * scalar);
            const Vector4 next_mask_transform(viewport_offset.x, viewport_offset.y, scalar, 0);
            if (!state.mask_initialized) {
                state.atlas_rect = next_atlas_rect;
                state.mask_transform = next_mask_transform;
                state.mask_initialized = true;
            } else {
                // A SubViewportTexture contains the mask rendered during the
                // previous frame. Keep its sampling transform one update behind
                // the mask nodes as well, otherwise moving masks and their
                // consumers visibly separate during animation.
                state.pending_atlas_rect = next_atlas_rect;
                state.pending_mask_transform = next_mask_transform;
                state.mask_pending = true;
            }
        }
    }
    }

    // One small parameter upload replaces per-draw uniforms in batched geometry.
    PackedByteArray parameters;
    const int parameter_rows = Math::max(1, (model->GetDrawableCount() * 5 + 1023) / 1024);
    parameters.resize(1024 * parameter_rows * 4 * sizeof(float));
    float *parameter_data = reinterpret_cast<float *>(parameters.ptrw());
    for (int i = 0; i < model->GetDrawableCount(); ++i) {
        const auto &s = res.drawables[i];
        const Vector4 values[] = {s.base, s.screen, s.multiply, s.atlas_rect, s.mask_transform};
        for (int j = 0; j < 5; ++j)
            for (int k = 0; k < 4; ++k) parameter_data[(i * 5 + j) * 4 + k] = values[j][k];
    }
    res.parameter_texture->update(Image::create_from_data(1024, parameter_rows, false, Image::FORMAT_RGBAF, parameters));
    for (auto &state : res.drawables) if (state.mask_pending) {
        state.atlas_rect = state.pending_atlas_rect;
        state.mask_transform = state.pending_mask_transform;
        state.mask_pending = false;
    }

    std::vector<int> order;
    for (int i = 0; i < model->GetDrawableCount(); ++i)
        if (res.drawables[i].node) order.push_back(i);
    std::stable_sort(order.begin(), order.end(), [&](int a, int b) { return renderOrder[a] < renderOrder[b]; });
    std::vector<int> signature;
    signature.reserve(order.size() * 2);
    for (int i : order) { signature.push_back(i); signature.push_back(renderOrder[i]); }
    if (!res.batching) {
        if (signature != res.batch_signature) {
            for (int i : order) {
                auto *node = res.drawables[i].node;
                node->set_z_index(0);
                node->get_parent()->move_child(node, -1);
            }
            res.batch_signature = signature;
        }
        return;
    }
    if (signature != res.batch_signature) {
        for (auto *batch : res.batches) batch->set_visible(false);
        size_t batch_index = 0;
        for (size_t begin = 0; begin < order.size();) {
            const int first = order[begin];
            const auto shader = res.drawables[first].material->get_shader();
            const int texture = model->GetDrawableTextureIndex(first);
            size_t end = begin + 1;
            while (end < order.size() && res.drawables[order[end]].material->get_shader() == shader && model->GetDrawableTextureIndex(order[end]) == texture) ++end;
            PackedVector2Array vertices, uvs;
            PackedInt32Array indices;
            int vertex_count = 0, index_count = 0;
            for (size_t at = begin; at < end; ++at) {
                vertex_count += model->GetDrawableVertexCount(order[at]);
                index_count += model->GetDrawableVertexIndexCount(order[at]);
            }
            vertices.resize(vertex_count); uvs.resize(vertex_count); indices.resize(index_count);
            Vector2 *vertex_data = vertices.ptrw();
            Vector2 *uv_data = uvs.ptrw();
            int32_t *index_data = indices.ptrw();
            int vertex_offset = 0, index_offset = 0;
            for (size_t at = begin; at < end; ++at) {
                const int i = order[at];
                const int offset = vertex_offset;
                const auto *uv = model->GetDrawableVertexUvs(i);
                for (int v = 0; v < model->GetDrawableVertexCount(i); ++v) {
                    vertex_data[vertex_offset] = Vector2(res.vertex_offsets[i] + v, 0);
                    uv_data[vertex_offset++] = Vector2(uv[v].X, 1.0f - uv[v].Y);
                }
                const auto *ix = model->GetDrawableVertexIndices(i);
                for (int j = 0; j < model->GetDrawableVertexIndexCount(i); ++j) index_data[index_offset++] = offset + ix[j];
            }
            MeshInstance2D *batch;
            if (batch_index < res.batches.size()) batch = res.batches[batch_index];
            else {
                batch = memnew(MeshInstance2D);
                // Batches are created lazily on the first renderer update. The
                // owner may already have been assigned to a non-default canvas
                // visibility layer by its host (for example a subject-only
                // SubViewport), so copy that layer instead of leaving the new
                // CanvasItem on layer 1.
                batch->set_visibility_layer(res._owner_viewport->get_visibility_layer());
                res.render_root->add_child(batch);
                res.managed_nodes.append(batch);
                res.batches.push_back(batch);
            }
            Ref<ArrayMesh> mesh; mesh.instantiate();
            Array arrays; arrays.resize(Mesh::ARRAY_MAX);
            arrays[Mesh::ARRAY_VERTEX] = vertices; arrays[Mesh::ARRAY_TEX_UV] = uvs; arrays[Mesh::ARRAY_INDEX] = indices;
            mesh->add_surface_from_arrays(Mesh::PRIMITIVE_TRIANGLES, arrays);
            batch->set_mesh(mesh);
            batch->set_material(res.drawables[first].material);
            batch->set_z_index(0);
            // Reused batches must follow current render order too. Only do this
            // when the signature changes, not for every drawable every frame.
            batch->get_parent()->move_child(batch, -1);
            batch->set_visible(true);
            ++batch_index; begin = end;
        }
        res.batch_signature = signature;
    }
    AABB all_bounds;
    bool first_bounds = true;
    for (const auto &state : res.drawables) if (state.node) {
        all_bounds = first_bounds ? state.mesh->get_custom_aabb() : all_bounds.merge(state.mesh->get_custom_aabb());
        first_bounds = false;
    }
    for (auto *batch : res.batches) {
        Ref<ArrayMesh> mesh = batch->get_mesh();
        mesh->set_custom_aabb(all_bounds);
        RenderingServer::get_singleton()->canvas_item_set_custom_rect(batch->get_canvas_item(), true, Rect2(all_bounds.position.x, all_bounds.position.y, all_bounds.size.x, all_bounds.size.y));
    }
}

void InternalCubismRenderer2D::build_model(InternalCubismRendererResource &res, Node* target_node)
{
    const CubismModel *model = this->GetModel();
    // Keep internal reordering separate from user-owned attachments/effects.
    res.render_root = memnew(Node2D);
    res.render_root->set_name("CubismRenderRoot");
    res.render_root->set_visibility_layer(res._owner_viewport->get_visibility_layer());
    target_node->add_child(res.render_root);
    res.managed_nodes.append(res.render_root);
    res.batching = ProjectSettings::get_singleton()->get_setting("gd_cubism/rendering/batching", true);
    int total_vertices = 0;
    res.vertex_offsets.resize(model->GetDrawableCount());
    res.drawables.resize(model->GetDrawableCount());
    for (int i = 0; i < model->GetDrawableCount(); ++i) {
        res.vertex_offsets[i] = total_vertices;
        total_vertices += model->GetDrawableVertexCount(i);
    }
    res.position_rows = Math::max(1, (total_vertices + 1023) / 1024);
    res.position_texture = ImageTexture::create_from_image(Image::create(1024, res.position_rows, false, Image::FORMAT_RGBAF));
    res.parameter_texture = ImageTexture::create_from_image(Image::create(1024, Math::max(1, (model->GetDrawableCount() * 5 + 1023) / 1024), false, Image::FORMAT_RGBAF));
    const Csm::csmInt32 *renderOrder = model->GetRenderOrders();
    const Csm::csmInt32 *maskCount = model->GetDrawableMaskCounts();

    this->make_ArrayMesh_prepare(
        model,
        res);

    Array meshes;
    meshes.resize(model->GetDrawableCount());

    for (Csm::csmInt32 index = 0; index < model->GetDrawableCount(); index++)
    {
        if (model->GetDrawableVertexCount(index) == 0)
            continue;
        if (model->GetDrawableVertexIndexCount(index) == 0)
            continue;

        CubismIdHandle handle = model->GetDrawableId(index);
        String node_name(handle->GetString().GetRawString());

        MeshInstance2D* node = res.request_mesh_instance();
        // share drawable mesh between nodes and masks so we only have to update once
        if (meshes[index]) {
            node->set_mesh(meshes[index]);
        } else {
            meshes[index] = node->get_mesh();
            this->update_mesh(model, index, res, node->get_mesh());
        }
        
        ShaderMaterial* mat = res.request_shader_material(model, index);
        node->set_material(mat);        
        node->set_name(node_name);
        node->set_meta("drawable_index", index);
        res.drawables[index].node = node;
        res.drawables[index].material = Ref<ShaderMaterial>(mat);
        res.drawables[index].mesh = node->get_mesh();

        res.dict_mesh[node_name] = node;
        res.render_root->add_child(node);
        res.managed_nodes.append(node);

        // node has a mask
        if (model->GetDrawableMaskCounts()[index] <= 0)
            continue;
        
        // calculate name based on referenced art mesh names composing the mask
        Array mask_names;
        for (Csm::csmInt32 m_index = 0; m_index < model->GetDrawableMaskCounts()[index]; m_index++)
        {
            Csm::csmInt32 j = model->GetDrawableMasks()[index][m_index];
            
            if (model->GetDrawableVertexCount(j) == 0)
                continue;
            if (model->GetDrawableVertexIndexCount(j) == 0)
                continue;
    
            CubismIdHandle handle = model->GetDrawableId(j);
            String mask_name(handle->GetString().GetRawString());
            mask_names.append(mask_name);
        }

        if (mask_names.is_empty())
            continue;

        // sort mask ids to gurantee consistency in hashing
        mask_names.sort();

        String mask_hash = String::num_int64(String("|").join(mask_names).hash());

        // tag mesh node as dependent on a mask if one has already been created with the same composition
        Array vp_meshes = res.dict_mask_meshes.get(mask_hash, Array());
        vp_meshes.append(node);

        // build a new mask
        if (!res.dict_mask.has(mask_hash)) {
            if (!res.mask_atlas) {
                res.mask_atlas = memnew(SubViewport);
                res.mask_atlas->set_name("CubismMaskAtlas");
                res.mask_atlas->set_disable_3d(true);
                res.mask_atlas->set_transparent_background(true);
                res.mask_atlas->set_disable_input(true);
                res.mask_atlas->set_update_mode(SubViewport::UPDATE_ALWAYS);
                res.mask_atlas->set_size(Vector2i(32, 32));
                target_node->add_child(res.mask_atlas);
                res.managed_nodes.append(res.mask_atlas);
            }
            Node2D* viewport = memnew(Node2D);

            res.dict_mask[mask_hash] = viewport;
            
            viewport->set_name(mask_hash + "__mask");


            for (Csm::csmInt32 m_index = 0; m_index < model->GetDrawableMaskCounts()[index]; m_index++)
            {
                Csm::csmInt32 j = model->GetDrawableMasks()[index][m_index];
                
                if (model->GetDrawableVertexCount(j) == 0)
                    continue;
                if (model->GetDrawableVertexIndexCount(j) == 0)
                    continue;
        
                CubismIdHandle handle = model->GetDrawableId(j);
                String mask_name(handle->GetString().GetRawString());

                MeshInstance2D* node = res.request_mesh_instance();
                if (meshes[j]) {
                    node->set_mesh(meshes[j]);
                } else {
                    meshes[j] = node->get_mesh();
                    this->update_mesh(model, j, res, node->get_mesh());
                }
                ShaderMaterial *mat = res.request_mask_material();

                node->set_name(mask_name);
                node->set_material(mat);
                mat->set_shader_parameter("channel", Vector4(0.0, 0.0, 0.0, 1.0));
                mat->set_shader_parameter("tex_main", res.ary_texture[model->GetDrawableTextureIndex(j)]);

                node->set_z_index(model->GetRenderOrders()[index]);
                node->set_visible(true);

                viewport->add_child(node);
                res.managed_nodes.append(node);
            }

            res.mask_atlas->add_child(viewport);
            res.managed_nodes.append(viewport);

            node->set_meta("viewport", viewport);

            // canvas transform only available after the viewport canvas has been initialized
            // on load the mask will not be the right size or offset, but will be corrected immediately on first update
            Vector2i viewport_size = Vector2i(1,1);
            Vector2 viewport_offset = Vector2(0,0);

            mat->set_shader_parameter("tex_mask", res.mask_atlas->get_texture());
            mat->set_shader_parameter("canvas_size", Vector2(res.vct_canvas_size));
            mat->set_shader_parameter("mesh_offset", viewport_offset);
        }

        res.dict_mask_meshes[mask_hash] = vp_meshes;
    }
    if (res.mask_atlas) for (auto &state : res.drawables)
        if (state.material.is_valid()) state.material->set_shader_parameter("tex_mask", res.mask_atlas->get_texture());
}

void InternalCubismRenderer2D::Initialize(Csm::CubismModel *model, Csm::csmInt32 maskBufferCount)
{
    CubismRenderer::Initialize(model, maskBufferCount);
}

void InternalCubismRenderer2D::DoDrawModel() {}
void InternalCubismRenderer2D::SaveProfile() {}
void InternalCubismRenderer2D::RestoreProfile() {}
void InternalCubismRenderer2D::BeforeDrawModelRenderTarget() {}
void InternalCubismRenderer2D::AfterDrawModelRenderTarget() {}

// ------------------------------------------------------------------ method(s)
PackedInt32Array make_Indices(const csmUint16 *ptr, const int32_t &size)
{
    PackedInt32Array ary;
    ary.resize(size);
    for (int i = 0; i < size; i++)
    {
        ary.set(i, ptr[i]);
    }
    return ary;
}

PackedVector2Array make_UVs(const Live2D::Cubism::Core::csmVector2 *ptr, const int32_t &size)
{
    PackedVector2Array ary;
    ary.resize(size);
    for (int i = 0; i < size; i++)
    {
        ary.set(i, Vector2(ptr[i].X, 1.0 - ptr[i].Y));
    }
    return ary;
}

PackedVector2Array make_Vertices(const Live2D::Cubism::Core::csmVector2 *ptr, const int32_t &size, const Csm::csmFloat32 &ppunit)
{
    PackedVector2Array ary;
    ary.resize(size);
    for (int i = 0; i < size; i++)
    {
        ary.set(i, Vector2(ptr[i].X, ptr[i].Y * -1.0) * ppunit);
    }
    return ary;
}

const Vector4 make_vector4(const Live2D::Cubism::Core::csmVector4 &src_vec4)
{
    return Vector4(src_vec4.X, src_vec4.Y, src_vec4.Z, src_vec4.W);
}

#endif // GD_CUBISM_USE_RENDERER_2D
