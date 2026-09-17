extends SceneTree

const MODEL := "res://assets/live2d/mao/runtime/mao_pro.model3.json"


func _initialize() -> void:
	_run.call_deferred()


func _run() -> void:
	ProjectSettings.set_setting("gd_cubism/rendering/batching", false)
	var model := GDCubismUserModel.new()
	root.add_child(model)
	model.assets = MODEL
	model.playback_process_mode = GDCubismUserModel.MANUAL

	var version: int = model.csm_get_version().version
	assert(
		version == 0x06000001 or version == 0x05010000,
		"Test is not running against the Purism Core ABI provider (got version 0x%08x)" % version
	)
	assert(model.get_parameters().size() == 128, "Purism Core parameter ABI mismatch")
	assert(model.get_meshes().size() == 260, "Purism Core drawable ABI mismatch")
	assert(not model.get_canvas_info().is_empty(), "Purism Core canvas ABI mismatch")
	assert(
		model.get_canvas_info().size_in_pixels.is_equal_approx(Vector2(5800, 8400)),
		"Canvas unit conversion mismatch: %s" % model.get_canvas_info().size_in_pixels
	)

	var angle_x: GDCubismParameter
	for parameter in model.get_parameters():
		if parameter.id == "ParamAngleX":
			angle_x = parameter
			break
	assert(angle_x != null, "ParamAngleX missing through Cubism Core ABI")
	angle_x.value = 20.0
	model.advance(1.0 / 60.0)
	assert(is_equal_approx(angle_x.value, 20.0), "Parameter write did not survive Core update")

	var masked_drawables := 0
	for child in model.get_children():
		if child is SubViewport:
			masked_drawables += 1
	assert(masked_drawables > 0, "Purism Core did not expose clipping masks")

	var drawable_surfaces := 0
	for mesh_instance in model.get_meshes().values():
		if mesh_instance != null and mesh_instance.mesh != null and mesh_instance.mesh.get_surface_count() > 0:
			drawable_surfaces += 1
	assert(drawable_surfaces > 200, "Purism-backed gd_cubism did not build drawable geometry")

	print(
		"PURISM_CORE_ABI_TEST_OK parameters=%d drawables=%d surfaces=%d mask_viewports=%d canvas=%s"
		% [
			model.get_parameters().size(),
			model.get_meshes().size(),
			drawable_surfaces,
			masked_drawables,
			model.get_canvas_info().size_in_pixels,
		]
	)
	ProjectSettings.set_setting("gd_cubism/rendering/batching", true)
	quit(0)
