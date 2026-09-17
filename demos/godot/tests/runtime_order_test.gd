extends SceneTree

func _initialize() -> void:
	run.call_deferred()

func run() -> void:
	for batching in [false, true]:
		ProjectSettings.set_setting("gd_cubism/rendering/batching", batching)
		var viewport := SubViewport.new()
		viewport.size = Vector2i(320, 360)
		viewport.render_target_update_mode = SubViewport.UPDATE_ALWAYS
		root.add_child(viewport)
		var model := GDCubismUserModel.new()
		viewport.add_child(model)
		model.assets = "res://assets/live2d/mao/runtime/mao_pro.model3.json"
		model.position = Vector2(160, 180)
		model.scale = Vector2.ONE * 0.07
		model.physics_evaluate = false
		var cover := ColorRect.new()
		cover.size = Vector2(320, 360)
		cover.color = Color.GREEN
		viewport.add_child(cover)
		for expression in model.get_expressions():
			model.start_expression(expression)
			for frame in 10:
				await process_frame
			await RenderingServer.frame_post_draw
			var pixels := viewport.get_texture().get_image()
			for y in range(0, 360, 4):
				for x in range(0, 320, 4):
					var pixel := pixels.get_pixel(x, y)
					if pixel.r > 0.02 or pixel.g < 0.98 or pixel.b > 0.02:
						push_error("Drawable escaped model-local order: batching=%s" % batching)
						quit(1)
						return
		cover.hide()
		await process_frame
		await RenderingServer.frame_post_draw
		var visible_pixels := viewport.get_texture().get_image()
		var colored := 0
		for y in range(0, 360, 4):
			for x in range(0, 320, 4):
				var pixel := visible_pixels.get_pixel(x, y)
				if pixel.r + pixel.g + pixel.b > 0.5:
					colored += 1
		assert(colored > 100, "Model must actually render for ordering test")
		viewport.queue_free()
		await process_frame
	ProjectSettings.set_setting("gd_cubism/rendering/batching", true)
	print("RUNTIME_ORDER_TEST_OK")
	quit()
