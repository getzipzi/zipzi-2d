extends SceneTree

func _initialize() -> void:
	run.call_deferred()

func run() -> void:
	var model := GDCubismUserModel.new()
	root.add_child(model)
	model.assets = "res://assets/live2d/mao/runtime/mao_pro.model3.json"
	model.position = Vector2(300, 300)
	model.scale = Vector2.ONE * 0.04
	model.playback_process_mode = GDCubismUserModel.IDLE
	model.start_motion_loop("Idle", 0, GDCubismUserModel.PRIORITY_FORCE, true, true)
	for frame in 30:
		await process_frame
	var masks: Array[SubViewport] = []
	for child in model.get_children():
		if child is SubViewport:
			masks.append(child)
	assert(not masks.is_empty(), "Model must exercise masks")
	var pixels := 0
	for mask in masks:
		pixels += mask.size.x * mask.size.y
	assert(pixels < 1000000, "Small on-screen model must not allocate authoring-resolution masks")
	model.position = Vector2(-10000, -10000)
	for frame in 3:
		await process_frame
	for mask in masks:
		assert(mask.render_target_update_mode == SubViewport.UPDATE_DISABLED)
	model.position = Vector2(300, 300)
	model.scale = Vector2(-0.08, 0.08)
	model.rotation = 0.2
	for frame in 3:
		await process_frame
	var active := 0
	for mask in masks:
		if mask.render_target_update_mode == SubViewport.UPDATE_ALWAYS:
			active += 1
	assert(active > 0, "Mirrored/rotated model must resume masks")
	print("MASK_ALLOCATION_TEST_OK masks=%d small_model_pixels=%d" % [masks.size(), pixels])
	quit()
