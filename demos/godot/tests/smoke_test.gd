extends SceneTree


func _initialize() -> void:
	_run.call_deferred()


func _run() -> void:
	var packed_scene: PackedScene = load("res://main.tscn")
	var demo = packed_scene.instantiate()
	root.add_child(demo)
	await process_frame
	await process_frame

	assert(demo.model.assets != "", "Live2D model did not load")
	assert(not demo.model.get_canvas_info().is_empty(), "Live2D canvas metadata is empty")
	assert(demo.motions.size() == 7, "Expected 7 sample motions")
	assert(demo.expressions.size() == 8, "Expected 8 sample expressions")

	demo.motion_select.select(1)
	demo._on_play_motion_pressed()
	demo.expression_select.select(1)
	demo._on_apply_expression_pressed()
	demo._on_clear_expression_pressed()

	print("SMOKE_TEST_OK: model loaded; 7 motions; 8 expressions; controls callable")
	quit(0)

