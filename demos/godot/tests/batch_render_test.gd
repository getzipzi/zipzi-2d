extends SceneTree

func _initialize() -> void:
	run.call_deferred()

func run() -> void:
	var viewports: Array[SubViewport] = []
	var models: Array[GDCubismUserModel] = []
	for batching in [false, true]:
		ProjectSettings.set_setting("gd_cubism/rendering/batching", batching)
		var viewport := SubViewport.new()
		viewport.size = Vector2i(640, 720)
		viewport.disable_3d = true
		viewport.render_target_update_mode = SubViewport.UPDATE_ALWAYS
		root.add_child(viewport)
		var model := GDCubismUserModel.new()
		viewport.add_child(model)
		model.assets = "res://assets/live2d/mao/runtime/mao_pro.model3.json"
		model.position = Vector2(320, 370)
		model.scale = Vector2.ONE * 0.14
		model.physics_evaluate = false
		model.pose_update = true
		model.playback_process_mode = GDCubismUserModel.IDLE
		viewports.append(viewport)
		models.append(model)
	ProjectSettings.set_setting("gd_cubism/rendering/batching", true)
	var expressions: Array = [""]
	expressions.append_array(models[0].get_expressions())
	for expression in expressions:
		if expression != "":
			for model in models:
				model.start_expression(expression)
		for frame in 20:
			await process_frame
		await RenderingServer.frame_post_draw
		var reference := viewports[0].get_texture().get_image()
		var actual := viewports[1].get_texture().get_image()
		var differences := 0
		for y in range(0, 720, 2):
			for x in range(0, 640, 2):
				var a := reference.get_pixel(x, y)
				var b := actual.get_pixel(x, y)
				if absf(a.r-b.r) + absf(a.g-b.g) + absf(a.b-b.b) > 0.08:
					differences += 1
		print("BATCH_RENDER_COMPARE expression=%s differences=%d" % [expression, differences])
		if differences > 1152:
			reference.save_png("res://artifacts/benchmarks/reference-failure.png")
			actual.save_png("res://artifacts/benchmarks/batch-failure.png")
			push_error("Batch rendering differs on more than 1% of sampled pixels")
			quit(1)
			return
	viewports[1].get_texture().get_image().save_png("res://artifacts/benchmarks/batch-verified.png")
	print("BATCH_RENDER_TEST_OK")
	quit()
