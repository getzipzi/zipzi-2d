// MOC3 v5 schema, derived from PurismCore src/moc3.h (MIT).
// Copyright (c) 2026 Sakura Motion Project
// Explicit wire widths; never sizeof(native pointer/struct).
SECTION("count_info", 256, -1) // 0: struct psm__count_info, 1
SECTION("canvas_info", 24, -1) // 1: struct psm__canvas_info, 1
SECTION("part_src.id_runtime", 8, 0) // 2: const char *, parts
SECTION("part_src.id", 64, 0) // 3: struct psm__id, parts
SECTION("part_src.binding_idx", 4, 0) // 4: psm__i32, parts
SECTION("part_src.keyform_off", 4, 0) // 5: psm__i32, parts
SECTION("part_src.key_len", 4, 0) // 6: psm__i32, parts
SECTION("part_src.visible", 4, 0) // 7: psm__i32, parts
SECTION("part_src.enable", 4, 0) // 8: psm__i32, parts
SECTION("part_src.parent_part_idx", 4, 0) // 9: psm__i32, parts
SECTION("deformer_src.id_runtime", 8, 1) // 10: const char *, deformers
SECTION("deformer_src.id", 64, 1) // 11: struct psm__id, deformers
SECTION("deformer_src.binding_idx", 4, 1) // 12: psm__i32, deformers
SECTION("deformer_src.visible", 4, 1) // 13: psm__i32, deformers
SECTION("deformer_src.enable", 4, 1) // 14: psm__i32, deformers
SECTION("deformer_src.parent_part_idx", 4, 1) // 15: psm__i32, deformers
SECTION("deformer_src.parent_deformer_idx", 4, 1) // 16: psm__i32, deformers
SECTION("deformer_src.type", 4, 1) // 17: psm__i32, deformers
SECTION("deformer_src.local_idx", 4, 1) // 18: psm__i32, deformers
SECTION("warp_src.binding_idx", 4, 2) // 19: psm__i32, warps
SECTION("warp_src.keyform_off", 4, 2) // 20: psm__i32, warps
SECTION("warp_src.key_len", 4, 2) // 21: psm__i32, warps
SECTION("warp_src.vertex_count", 4, 2) // 22: psm__i32, warps
SECTION("warp_src.row", 4, 2) // 23: psm__i32, warps
SECTION("warp_src.col", 4, 2) // 24: psm__i32, warps
SECTION("rotation_src.binding_idx", 4, 3) // 25: psm__i32, rotations
SECTION("rotation_src.keyform_off", 4, 3) // 26: psm__i32, rotations
SECTION("rotation_src.key_len", 4, 3) // 27: psm__i32, rotations
SECTION("rotation_src.base_angle", 4, 3) // 28: psm__f32, rotations
SECTION("art_mesh_src.id_runtime", 8, 4) // 29: const char *, art_meshes
SECTION("art_mesh_src.uv_runtime", 8, 4) // 30: const psm__f32 *, art_meshes
SECTION("art_mesh_src.pos_idx_runtime", 8, 4) // 31: const psm__u16 *, art_meshes
SECTION("art_mesh_src.drawable_mask_runtime", 8, 4) // 32: const psm__i32 *, art_meshes
SECTION("art_mesh_src.id", 64, 4) // 33: struct psm__id, art_meshes
SECTION("art_mesh_src.binding_idx", 4, 4) // 34: psm__i32, art_meshes
SECTION("art_mesh_src.keyform_off", 4, 4) // 35: psm__i32, art_meshes
SECTION("art_mesh_src.key_len", 4, 4) // 36: psm__i32, art_meshes
SECTION("art_mesh_src.visible", 4, 4) // 37: psm__i32, art_meshes
SECTION("art_mesh_src.enable", 4, 4) // 38: psm__i32, art_meshes
SECTION("art_mesh_src.parent_part_idx", 4, 4) // 39: psm__i32, art_meshes
SECTION("art_mesh_src.parent_deformer_idx", 4, 4) // 40: psm__i32, art_meshes
SECTION("art_mesh_src.texture_no", 4, 4) // 41: psm__i32, art_meshes
SECTION("art_mesh_src.drawable_flag", 1, 4) // 42: psm__u8, art_meshes
SECTION("art_mesh_src.vertex_count", 4, 4) // 43: psm__i32, art_meshes
SECTION("art_mesh_src.uv_off", 4, 4) // 44: psm__i32, art_meshes
SECTION("art_mesh_src.idx_off", 4, 4) // 45: psm__i32, art_meshes
SECTION("art_mesh_src.idx_len", 4, 4) // 46: psm__i32, art_meshes
SECTION("art_mesh_src.mask_off", 4, 4) // 47: psm__i32, art_meshes
SECTION("art_mesh_src.mask_len", 4, 4) // 48: psm__i32, art_meshes
SECTION("param_src.id_runtime", 8, 5) // 49: const char *, parameters
SECTION("param_src.id", 64, 5) // 50: struct psm__id, parameters
SECTION("param_src.maximum_value", 4, 5) // 51: psm__f32, parameters
SECTION("param_src.minimum_value", 4, 5) // 52: psm__f32, parameters
SECTION("param_src.default_value", 4, 5) // 53: psm__f32, parameters
SECTION("param_src.repeat", 4, 5) // 54: psm__i32, parameters
SECTION("param_src.decimal_places", 4, 5) // 55: psm__i32, parameters
SECTION("param_src.key_table_off", 4, 5) // 56: psm__i32, parameters
SECTION("param_src.key_table_len", 4, 5) // 57: psm__i32, parameters
SECTION("part_key_src.draw_order", 4, 6) // 58: psm__f32, part_keyforms
SECTION("warp_key_src.opacity", 4, 7) // 59: psm__f32, warp_keyforms
SECTION("warp_key_src.key_pos_off", 4, 7) // 60: psm__i32, warp_keyforms
SECTION("rotation_key_src.opacity", 4, 8) // 61: psm__f32, rotation_keyforms
SECTION("rotation_key_src.angle", 4, 8) // 62: psm__f32, rotation_keyforms
SECTION("rotation_key_src.origin_x", 4, 8) // 63: psm__f32, rotation_keyforms
SECTION("rotation_key_src.origin_y", 4, 8) // 64: psm__f32, rotation_keyforms
SECTION("rotation_key_src.scale", 4, 8) // 65: psm__f32, rotation_keyforms
SECTION("rotation_key_src.reflect_x", 4, 8) // 66: psm__i32, rotation_keyforms
SECTION("rotation_key_src.reflect_y", 4, 8) // 67: psm__i32, rotation_keyforms
SECTION("art_mesh_key_src.opacity", 4, 9) // 68: psm__f32, art_mesh_keyforms
SECTION("art_mesh_key_src.draw_order", 4, 9) // 69: psm__f32, art_mesh_keyforms
SECTION("art_mesh_key_src.key_pos_off", 4, 9) // 70: psm__i32, art_mesh_keyforms
SECTION("key_pos_src.xy", 4, 10) // 71: psm__f32, keyform_pos
SECTION("key_table_idx_src.idx", 4, 11) // 72: psm__i32, key_table_idx
SECTION("binding_src.key_table_idx_off", 4, 12) // 73: psm__i32, bindings
SECTION("binding_src.key_table_idx_len", 4, 12) // 74: psm__i32, bindings
SECTION("key_table_src.keys_off", 4, 13) // 75: psm__i32, key_tables
SECTION("key_table_src.keys_len", 4, 13) // 76: psm__i32, key_tables
SECTION("keys_src.key", 4, 14) // 77: psm__f32, keys
SECTION("uv_src.xy", 4, 15) // 78: psm__f32, uvs
SECTION("idx_src.idx", 2, 16) // 79: psm__u16, idx
SECTION("mask_src.art_mesh_idx", 4, 17) // 80: psm__i32, masks
SECTION("draw_group_src.obj_off", 4, 18) // 81: psm__i32, draw_groups
SECTION("draw_group_src.obj_len", 4, 18) // 82: psm__i32, draw_groups
SECTION("draw_group_src.obj_total_count", 4, 18) // 83: psm__i32, draw_groups
SECTION("draw_group_src.max_order", 4, 18) // 84: psm__i32, draw_groups
SECTION("draw_group_src.min_order", 4, 18) // 85: psm__i32, draw_groups
SECTION("draw_group_obj_src.type", 4, 19) // 86: psm__i32, draw_items
SECTION("draw_group_obj_src.idx", 4, 19) // 87: psm__i32, draw_items
SECTION("draw_group_obj_src.self_group_idx", 4, 19) // 88: psm__i32, draw_items
SECTION("glue_src.id_runtime", 8, 20) // 89: const char *, glues
SECTION("glue_src.id", 64, 20) // 90: struct psm__id, glues
SECTION("glue_src.binding_idx", 4, 20) // 91: psm__i32, glues
SECTION("glue_src.keyform_off", 4, 20) // 92: psm__i32, glues
SECTION("glue_src.key_len", 4, 20) // 93: psm__i32, glues
SECTION("glue_src.art_mesh_idx_a", 4, 20) // 94: psm__i32, glues
SECTION("glue_src.art_mesh_idx_b", 4, 20) // 95: psm__i32, glues
SECTION("glue_src.info_off", 4, 20) // 96: psm__i32, glues
SECTION("glue_src.info_len", 4, 20) // 97: psm__i32, glues
SECTION("glue_info_src.weight", 4, 21) // 98: psm__f32, glue_info
SECTION("glue_info_src.pos_idx", 2, 21) // 99: psm__u16, glue_info
SECTION("glue_key_src.intensity", 4, 22) // 100: psm__f32, glue_keyforms
SECTION("warp_src.quad_transform", 4, 2) // 101: psm__i32, warps
SECTION("param_keys_src.key_runtime", 8, 5) // 102: const psm__f32 *, parameters
SECTION("param_keys_src.keys_off", 4, 5) // 103: psm__i32, parameters
SECTION("param_keys_src.keys_len", 4, 5) // 104: psm__i32, parameters
SECTION("warp_src.key_color_off", 4, 2) // 105: psm__i32, warps
SECTION("rotation_src.key_color_off", 4, 3) // 106: psm__i32, rotations
SECTION("art_mesh_src.key_color_off", 4, 4) // 107: psm__i32, art_meshes
SECTION("keyform_mul_color_src.r", 4, 23) // 108: psm__f32, keyform_mul_colors
SECTION("keyform_mul_color_src.g", 4, 23) // 109: psm__f32, keyform_mul_colors
SECTION("keyform_mul_color_src.b", 4, 23) // 110: psm__f32, keyform_mul_colors
SECTION("keyform_scr_color_src.r", 4, 24) // 111: psm__f32, keyform_scr_colors
SECTION("keyform_scr_color_src.g", 4, 24) // 112: psm__f32, keyform_scr_colors
SECTION("keyform_scr_color_src.b", 4, 24) // 113: psm__f32, keyform_scr_colors
SECTION("param_src.type", 4, 5) // 114: psm__i32, parameters
SECTION("param_src.blend_key_table_off", 4, 5) // 115: psm__i32, parameters
SECTION("param_src.blend_key_table_len", 4, 5) // 116: psm__i32, parameters
SECTION("blend_key_table_src.keys_off", 4, 25) // 117: psm__i32, blend_key_tables
SECTION("blend_key_table_src.keys_len", 4, 25) // 118: psm__i32, blend_key_tables
SECTION("blend_key_table_src.base_key_idx", 4, 25) // 119: psm__i32, blend_key_tables
SECTION("blend_binding_src.key_table_idx", 4, 26) // 120: psm__i32, blend_bindings
SECTION("blend_binding_src.key_bs_off", 4, 26) // 121: psm__i32, blend_bindings
SECTION("blend_binding_src.key_bs_len", 4, 26) // 122: psm__i32, blend_bindings
SECTION("blend_binding_src.bs_constraint_idx_off", 4, 26) // 123: psm__i32, blend_bindings
SECTION("blend_binding_src.bs_constraint_idx_len", 4, 26) // 124: psm__i32, blend_bindings
SECTION("bs_warp_src.target_idx", 4, 27) // 125: psm__i32, bs_warps
SECTION("bs_warp_src.bs_binding_off", 4, 27) // 126: psm__i32, bs_warps
SECTION("bs_warp_src.bs_binding_len", 4, 27) // 127: psm__i32, bs_warps
SECTION("bs_art_mesh_src.target_idx", 4, 28) // 128: psm__i32, bs_art_meshes
SECTION("bs_art_mesh_src.bs_binding_off", 4, 28) // 129: psm__i32, bs_art_meshes
SECTION("bs_art_mesh_src.bs_binding_len", 4, 28) // 130: psm__i32, bs_art_meshes
SECTION("blend_constraint_idx_src.constraint_idx", 4, 29) // 131: psm__i32, bs_constraint_idx
SECTION("blend_constraint_src.parameter_idx", 4, 30) // 132: psm__i32, bs_constraints
SECTION("blend_constraint_src.value_off", 4, 30) // 133: psm__i32, bs_constraints
SECTION("blend_constraint_src.value_len", 4, 30) // 134: psm__i32, bs_constraints
SECTION("blend_constraint_val_src.key", 4, 31) // 135: psm__f32, bs_constraint_vals
SECTION("blend_constraint_val_src.weight", 4, 31) // 136: psm__f32, bs_constraint_vals
SECTION("warp_key_src.key_mul_color_off", 4, 7) // 137: psm__i32, warp_keyforms
SECTION("warp_key_src.key_scr_color_off", 4, 7) // 138: psm__i32, warp_keyforms
SECTION("rotation_key_src.key_mul_color_off", 4, 8) // 139: psm__i32, rotation_keyforms
SECTION("rotation_key_src.key_scr_color_off", 4, 8) // 140: psm__i32, rotation_keyforms
SECTION("art_mesh_key_src.key_mul_color_off", 4, 9) // 141: psm__i32, art_mesh_keyforms
SECTION("art_mesh_key_src.key_scr_color_off", 4, 9) // 142: psm__i32, art_mesh_keyforms
SECTION("bs_part_src.target_idx", 4, 32) // 143: psm__i32, bs_parts
SECTION("bs_part_src.bs_binding_off", 4, 32) // 144: psm__i32, bs_parts
SECTION("bs_part_src.bs_binding_len", 4, 32) // 145: psm__i32, bs_parts
SECTION("bs_rotation_src.target_idx", 4, 33) // 146: psm__i32, bs_rotations
SECTION("bs_rotation_src.bs_binding_off", 4, 33) // 147: psm__i32, bs_rotations
SECTION("bs_rotation_src.bs_binding_len", 4, 33) // 148: psm__i32, bs_rotations
SECTION("bs_glue_src.target_idx", 4, 34) // 149: psm__i32, bs_glues
SECTION("bs_glue_src.bs_binding_off", 4, 34) // 150: psm__i32, bs_glues
SECTION("bs_glue_src.bs_binding_len", 4, 34) // 151: psm__i32, bs_glues
