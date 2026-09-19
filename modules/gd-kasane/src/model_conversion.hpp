// SPDX-License-Identifier: MIT
#pragma once
#include "conversions.hpp"
#include <godot_cpp/variant/array.hpp>
#include <godot_cpp/variant/packed_float32_array.hpp>
namespace kasane_gd {
kasane::Status parameter_from_dictionary(const godot::Dictionary &, kasane::Parameter &);
kasane::Status binding_from_dictionary(const godot::Dictionary &, kasane::MeshBinding &);
godot::Dictionary parameter_dictionary(const kasane::Parameter &);
godot::Dictionary binding_dictionary(const kasane::MeshBinding &);
}
