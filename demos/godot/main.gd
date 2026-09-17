extends Node2D

const MODEL_PATH := "res://assets/live2d/mao/runtime/mao_pro.model3.json"

@onready var model: GDCubismUserModel = $GDCubismUserModel
@onready var target_point: GDCubismEffectTargetPoint = $GDCubismUserModel/TargetPoint
@onready var background: ColorRect = $Background
@onready var motion_select: OptionButton = $UI/Panel/Margin/Controls/MotionSelect
@onready var expression_select: OptionButton = $UI/Panel/Margin/Controls/ExpressionSelect
@onready var status: Label = $UI/Panel/Margin/Controls/Status

var motions: Array[Dictionary] = []
var expressions: Array = []


func _ready() -> void:
	get_viewport().size_changed.connect(_layout_model)
	if not FileAccess.file_exists(MODEL_PATH):
		status.text = "Model not found:\n%s" % MODEL_PATH
		status.add_theme_color_override("font_color", Color(1.0, 0.42, 0.42))
		return

	model.assets = MODEL_PATH
	model.playback_process_mode = GDCubismUserModel.IDLE
	_build_motion_list()
	_build_expression_list()
	_layout_model()

	if not motions.is_empty():
		_play_motion(motions[0], true)

	status.text = "Live2D loaded successfully\n%d motions · %d expressions" % [motions.size(), expressions.size()]


func _process(_delta: float) -> void:
	var viewport_size := get_viewport_rect().size
	var pointer := get_viewport().get_mouse_position()
	var normalized := Vector2.ZERO
	var pointer_inside := Rect2(Vector2.ZERO, viewport_size).has_point(pointer)
	if get_window().has_focus() and pointer_inside:
		normalized = Vector2(
			clampf((pointer.x / maxf(viewport_size.x, 1.0)) * 2.0 - 1.0, -1.0, 1.0),
			clampf(-((pointer.y / maxf(viewport_size.y, 1.0)) * 2.0 - 1.0), -1.0, 1.0)
		)
	target_point.set_target(normalized)


func _unhandled_key_input(event: InputEvent) -> void:
	if event.is_action_pressed("ui_accept") and not motions.is_empty():
		var index := randi_range(0, motions.size() - 1)
		motion_select.select(index)
		_play_motion(motions[index], false)


func _build_motion_list() -> void:
	motions.clear()
	motion_select.clear()
	var available: Dictionary = model.get_motions()
	for group in available:
		for number in range(int(available[group])):
			var display_group := str(group) if str(group) != "" else "Special"
			var motion := {"group": str(group), "number": number}
			motions.append(motion)
			motion_select.add_item("%s %d" % [display_group, number + 1])


func _build_expression_list() -> void:
	expressions = model.get_expressions()
	expression_select.clear()
	for expression in expressions:
		expression_select.add_item(str(expression))


func _layout_model() -> void:
	var viewport_size := get_viewport_rect().size
	background.size = viewport_size
	var info: Dictionary = model.get_canvas_info()
	if info.is_empty():
		return

	var model_area := Vector2(maxf(viewport_size.x - 350.0, 100.0), viewport_size.y)
	var source_size: Vector2 = info.size_in_pixels
	var fit_scale := minf(model_area.x / source_size.x, (model_area.y * 0.92) / source_size.y)
	model.position = Vector2(model_area.x * 0.5, viewport_size.y * 0.52)
	model.scale = Vector2.ONE * fit_scale


func _play_motion(motion: Dictionary, loop: bool) -> void:
	model.start_motion_loop(
		motion.group,
		motion.number,
		GDCubismUserModel.PRIORITY_FORCE,
		loop,
		true
	)


func _on_play_motion_pressed() -> void:
	var index := motion_select.selected
	if index >= 0 and index < motions.size():
		_play_motion(motions[index], false)


func _on_apply_expression_pressed() -> void:
	var index := expression_select.selected
	if index >= 0 and index < expressions.size():
		model.start_expression(str(expressions[index]))


func _on_clear_expression_pressed() -> void:
	model.stop_expression()
