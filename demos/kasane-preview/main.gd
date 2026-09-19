extends Node2D

const Fixture = preload("res://fixture.gd")
var direct: KasaneMeshView
var document: KasaneDocumentBridge
var elapsed := 0.0
var stats: Label
var script_host: RefCounted
var script_path: LineEdit

func label_at(text: String, at: Vector2, size: int, color: Color) -> Label:
	var label := Label.new()
	label.text = text
	label.position = at
	label.add_theme_font_size_override("font_size", size)
	label.add_theme_color_override("font_color", color)
	add_child(label)
	return label

func _ready() -> void:
	var tex := Fixture.texture()
	var data := Fixture.mesh()
	direct = KasaneMeshView.new()
	direct.position = Vector2(250, 310)
	add_child(direct)
	var initialized := direct.initialize(data.base_positions, data.uvs, PackedInt32Array([0, 1, 2, 0, 2, 3]), tex)
	if not initialized.ok:
		push_error(str(initialized))
	document = KasaneDocumentBridge.new()
	document.position = Vector2(710, 310)
	add_child(document)
	var populated := Fixture.populate(document, tex)
	if not populated.ok:
		push_error(str(populated))
	script_host = preload("res://script_host.gd").new(document)
	script_path = LineEdit.new()
	script_path.position = Vector2(36, 510)
	script_path.size = Vector2(540, 30)
	script_path.text = "res://scripts/edit_document.gd"
	add_child(script_path)
	var run_button := Button.new()
	run_button.position = Vector2(590, 510)
	run_button.text = "Run script (F5)"
	run_button.pressed.connect(_run_script)
	add_child(run_button)
	var undo_button := Button.new()
	undo_button.position = Vector2(745, 510)
	undo_button.text = "Undo action"
	undo_button.pressed.connect(func(): print(script_host.actions.undo()))
	add_child(undo_button)
	var redo_button := Button.new()
	redo_button.position = Vector2(850, 510)
	redo_button.text = "Redo"
	redo_button.pressed.connect(func(): print(script_host.actions.redo()))
	add_child(redo_button)
	for argument in OS.get_cmdline_user_args():
		if argument.begins_with("--edit-script="):
			script_path.text = argument.trim_prefix("--edit-script=")
			_run_script.call_deferred()
	label_at("KASANE / MEMORY PREVIEW", Vector2(36, 28), 14, Color("7c91aa"))
	label_at("One mesh. Two paths. No model files.", Vector2(36, 57), 28, Color("eaf1fa"))
	label_at("STAGE 00", Vector2(66, 142), 13, Color("69baff"))
	label_at("Direct memory interface", Vector2(66, 166), 21, Color("eaf1fa"))
	label_at("STAGE 04", Vector2(526, 142), 13, Color("70dbc0"))
	label_at("GDScript → Document → preview", Vector2(526, 166), 20, Color("eaf1fa"))
	label_at("Fixed topology · shared texture · Y-up source data", Vector2(66, 466), 14, Color("9bacbf"))
	label_at("Direct data access · optional native actions", Vector2(526, 466), 14, Color("9bacbf"))
	stats = label_at("", Vector2(36, 544), 14, Color("9bacbf"))
	print("KASANE_PREVIEW_READY: ", document.get_document_summary())

func _process(delta: float) -> void:
	if document == null:
		return
	elapsed += delta
	var offset := sin(elapsed * 1.4) * 34.0
	var points: PackedVector2Array = Fixture.mesh().base_positions
	points[0] += Vector2(-offset * 0.25, offset)
	points[3] += Vector2(offset, offset * 0.5)
	direct.update_positions(points)
	var mesh_view := document.get_mesh_view(Fixture.MESH)
	if mesh_view == null:
		return
	var render := mesh_view.get_render_stats()
	stats.text = "IN-PROCESS SCRIPT   /   revision %d      vertex uploads %d      surface creations %d" % [document.get_document_summary().revision, render.position_uploads, render.surface_creations]

func _draw() -> void:
	for x in [46, 506]:
		draw_style_box(panel_style(), Rect2(x, 124, 408, 390))
		for gx in range(x + 24, x + 400, 24):
			draw_line(Vector2(gx, 210), Vector2(gx, 446), Color(0.16, 0.21, 0.28, 0.35))
		for gy in range(214, 447, 24):
			draw_line(Vector2(x + 18, gy), Vector2(x + 390, gy), Color(0.16, 0.21, 0.28, 0.35))

func panel_style() -> StyleBoxFlat:
	var style := StyleBoxFlat.new()
	style.bg_color = Color("101c2b")
	style.border_color = Color("26364a")
	style.set_border_width_all(1)
	style.set_corner_radius_all(12)
	return style

func _unhandled_key_input(event: InputEvent) -> void:
	if event is InputEventKey and event.pressed and not event.echo and event.keycode == KEY_F5:
		_run_script()

func _run_script() -> void:
	print("KASANE_SCRIPT_RESULT: ", script_host.run_script(script_path.text))
	var captured: Dictionary = await script_host.capture(get_viewport())
	if captured.ok:
		DirAccess.make_dir_recursive_absolute(ProjectSettings.globalize_path("res://artifacts"))
		captured.image.save_png("res://artifacts/script-preview.png")
		print("KASANE_SCRIPT_PREVIEW: revision ", captured.revision, " res://artifacts/script-preview.png")
