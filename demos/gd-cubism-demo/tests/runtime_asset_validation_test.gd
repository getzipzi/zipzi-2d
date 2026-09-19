extends SceneTree

func _initialize() -> void:
	run.call_deferred()

func run() -> void:
	var source := "res://assets/live2d/mao/runtime/mao_pro.model3.json"
	var settings: Dictionary = JSON.parse_string(FileAccess.get_file_as_string(source))
	var references: Dictionary = settings["FileReferences"]
	for key in ["Expressions", "Motions", "Physics", "Pose", "UserData"]:
		references.erase(key)
	var fixture := source.get_base_dir().path_join("runtime_validation_%d.model3.json" % OS.get_process_id())
	for textures in [["does-not-exist-runtime-test.png"], [""], null]:
		references["Textures"] = textures
		var file := FileAccess.open(fixture, FileAccess.WRITE)
		file.store_string("{invalid" if textures == null else JSON.stringify(settings, "\t"))
		file.close()
		var model := GDCubismUserModel.new()
		root.add_child(model)
		model.assets = fixture
		if not model.get_canvas_info().is_empty():
			push_error("Invalid texture accepted")
			quit(1)
			return
		model.queue_free()
		await process_frame
	DirAccess.remove_absolute(ProjectSettings.globalize_path(fixture))
	print("RUNTIME_ASSET_VALIDATION_TEST_OK (expected missing/empty texture and invalid JSON errors)")
	quit()
