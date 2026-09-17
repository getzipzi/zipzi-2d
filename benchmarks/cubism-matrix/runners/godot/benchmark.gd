extends Node2D

const WORKLOAD_PATH := "res://config/mao-40.json"

var _models: Array[GDCubismUserModel] = []
var _workload_id := ""
var _model_path := ""
var _model_name := ""
var _model_count := 0
var _columns := 0
var _rows := 0
var _cell_fill := 0.0
var _viewport_size := Vector2.ZERO
var _warmup_seconds := 0.0
var _sample_seconds := 0.0
var _motion_group := ""
var _motion_index := 0
var _case_id := "cubism-godot"
var _core_backend := "cubism"
var _build_profile := "unknown"
var _elapsed := 0.0
var _sampling := false
var _sample_started_usec := 0
var _frame_started_usec := 0
var _frame_times_ms: Array[float] = []
var _mask_sizes: Dictionary = {}
var _mask_resizes := 0


func _ready() -> void:
	if not _load_workload():
		get_tree().quit(2)
		return
	_apply_arguments()

	DisplayServer.window_set_vsync_mode(DisplayServer.VSYNC_DISABLED)
	Engine.max_fps = 0
	get_window().size = Vector2i(_viewport_size)

	for index in _model_count:
		var model := GDCubismUserModel.new()
		model.name = "Mao%02d" % index
		add_child(model)
		model.assets = _model_path
		model.playback_process_mode = GDCubismUserModel.IDLE
		_layout_model(model, index)
		model.start_motion_loop(
			_motion_group,
			_motion_index,
			GDCubismUserModel.PRIORITY_FORCE,
			true,
			true,
		)
		_models.append(model)

	print("BENCHMARK_READY case=%s renderer=%s models=%d size=%dx%d mipmaps=on warmup_s=%.1f sample_s=%.1f" % [
		_case_id, RenderingServer.get_current_rendering_driver_name(), _model_count,
		int(_viewport_size.x), int(_viewport_size.y), _warmup_seconds, _sample_seconds,
	])


func _load_workload() -> bool:
	var file := FileAccess.open(WORKLOAD_PATH, FileAccess.READ)
	if file == null:
		push_error("Cannot open benchmark workload: %s" % WORKLOAD_PATH)
		return false
	var parsed: Variant = JSON.parse_string(file.get_as_text())
	if not parsed is Dictionary:
		push_error("Benchmark workload is not a JSON object: %s" % WORKLOAD_PATH)
		return false
	var config: Dictionary = parsed
	var layout: Dictionary = config["layout"]
	var viewport: Array = config["viewport"]
	var timing: Dictionary = config["timing"]
	var motion: Dictionary = config["motion"]
	_workload_id = config["id"]
	_model_path = config["model_path"]
	_model_name = config["model"]
	_model_count = int(config["instances"])
	_columns = int(layout["columns"])
	_rows = int(layout["rows"])
	_cell_fill = float(layout["cell_fill"])
	_viewport_size = Vector2(float(viewport[0]), float(viewport[1]))
	_warmup_seconds = float(timing["warmup_seconds"])
	_sample_seconds = float(timing["sample_seconds"])
	_motion_group = motion["group"]
	_motion_index = int(motion["index"])
	return true


func _apply_arguments() -> void:
	for argument in OS.get_cmdline_user_args():
		if argument.begins_with("--case="):
			_case_id = argument.trim_prefix("--case=")
		if argument.begins_with("--core="):
			_core_backend = argument.trim_prefix("--core=")
		if argument.begins_with("--profile="):
			_build_profile = argument.trim_prefix("--profile=")
		if argument.begins_with("--models="):
			_model_count = maxi(int(argument.trim_prefix("--models=")), 1)
		if argument.begins_with("--seconds="):
			_sample_seconds = maxf(float(argument.trim_prefix("--seconds=")), 1.0)
	_columns = mini(_columns, _model_count)
	_rows = ceili(float(_model_count) / _columns)


func _process(delta: float) -> void:
	_elapsed += delta
	var now := Time.get_ticks_usec()
	for model in _models:
		for child in model.get_children():
			if child is SubViewport:
				var key := child.get_instance_id()
				if _sampling and _mask_sizes.has(key) and _mask_sizes[key] != child.size:
					_mask_resizes += 1
				_mask_sizes[key] = child.size

	if not _sampling:
		if _elapsed >= _warmup_seconds:
			_sampling = true
			_sample_started_usec = now
			_frame_started_usec = now
			print("BENCHMARK_SAMPLE_BEGIN")
		return

	if _frame_started_usec > 0:
		_frame_times_ms.append(float(now - _frame_started_usec) / 1000.0)
	_frame_started_usec = now

	if float(now - _sample_started_usec) >= _sample_seconds * 1_000_000.0:
		_finish_benchmark(now)


func _layout_model(model: GDCubismUserModel, index: int) -> void:
	var info: Dictionary = model.get_canvas_info()
	if info.is_empty():
		push_error("No canvas info for model %d" % index)
		return

	var cell_size := Vector2(_viewport_size.x / _columns, _viewport_size.y / _rows)
	var source_size: Vector2 = info.size_in_pixels
	var fit_scale := minf(
		(cell_size.x * _cell_fill) / source_size.x,
		(cell_size.y * _cell_fill) / source_size.y,
	)
	var column := index % _columns
	var row := index / _columns
	model.position = Vector2((column + 0.5) * cell_size.x, (row + 0.5) * cell_size.y)
	model.scale = Vector2.ONE * fit_scale


func _finish_benchmark(now_usec: int) -> void:
	set_process(false)
	_frame_times_ms.sort()
	var seconds := float(now_usec - _sample_started_usec) / 1_000_000.0
	var frames := _frame_times_ms.size()
	var average_fps := float(frames) / seconds
	var mask_pixels := 0
	for size in _mask_sizes.values():
		mask_pixels += size.x * size.y
	var result := {
		"schema_version": 1,
		"workload_id": _workload_id,
		"case_id": _case_id,
		"core_backend": _core_backend,
		"host_backend": "gd-cubism",
		"graphics_api": RenderingServer.get_current_rendering_driver_name(),
		"build_profile": _build_profile,
		"godot_version": Engine.get_version_info().get("string", "unknown"),
		"model": _model_name,
		"model_hash": FileAccess.get_sha256(_model_path),
		"instances": _model_count,
		"viewport": [int(_viewport_size.x), int(_viewport_size.y)],
		"texture_mipmaps": true,
		"warmup_seconds": _warmup_seconds,
		"sample_seconds": seconds,
		"frames": frames,
		"average_fps": average_fps,
		"mask_count": _mask_sizes.size(),
		"mask_pixels": mask_pixels,
		"mask_resizes_during_sample": _mask_resizes,
		"p50_frame_ms": _percentile(0.50),
		"p95_frame_ms": _percentile(0.95),
		"p99_frame_ms": _percentile(0.99),
		"render_objects": Performance.get_monitor(Performance.RENDER_TOTAL_OBJECTS_IN_FRAME),
		"draw_calls": Performance.get_monitor(Performance.RENDER_TOTAL_DRAW_CALLS_IN_FRAME),
		"video_memory_bytes": Performance.get_monitor(Performance.RENDER_VIDEO_MEM_USED),
	}
	print("BENCHMARK_RESULT %s" % JSON.stringify(result))
	DirAccess.make_dir_recursive_absolute(ProjectSettings.globalize_path("res://artifacts/results"))
	var result_file := FileAccess.open(
		"res://artifacts/results/latest-%s.json" % _case_id,
		FileAccess.WRITE,
	)
	if result_file:
		result_file.store_string(JSON.stringify(result, "\t"))
	if DisplayServer.get_name() != "headless":
		var screenshot := get_viewport().get_texture().get_image()
		if screenshot != null:
			screenshot.save_png("res://artifacts/results/latest-%s.png" % _case_id)
	get_tree().quit()


func _percentile(fraction: float) -> float:
	if _frame_times_ms.is_empty():
		return 0.0
	var index := mini(int(ceil(fraction * _frame_times_ms.size())) - 1, _frame_times_ms.size() - 1)
	return _frame_times_ms[maxi(index, 0)]
