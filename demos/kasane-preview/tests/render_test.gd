extends SceneTree

const Fixture = preload("res://fixture.gd")
var failures := 0

func check(condition: bool, message: String) -> void:
	if not condition:
		failures += 1
		push_error(message)

func _initialize() -> void:
	_run.call_deferred()

func capture(name: String) -> Image:
	await process_frame
	await RenderingServer.frame_post_draw
	var image := root.get_texture().get_image()
	check(image != null and not image.is_empty(), "Capture requires a real renderer")
	DirAccess.make_dir_recursive_absolute(ProjectSettings.globalize_path("res://artifacts"))
	check(image.save_png("res://artifacts/" + name + ".png") == OK, "Save render evidence")
	return image

func near_color(actual: Color, expected: Color) -> bool:
	return abs(actual.r - expected.r) < 0.06 and abs(actual.g - expected.g) < 0.06 and abs(actual.b - expected.b) < 0.06

func _run() -> void:
	root.content_scale_size = Vector2i(640, 360)
	root.size = Vector2i(640, 360)
	var tex := Fixture.texture()
	var data := Fixture.mesh()
	var direct := KasaneMeshView.new()
	direct.position = Vector2(140, 180)
	root.add_child(direct)
	check(direct.initialize(data.base_positions, data.uvs, PackedInt32Array([0, 1, 2, 0, 2, 3]), tex).ok, "Direct initialize")
	var bridge := KasaneDocumentBridge.new()
	bridge.position = Vector2(440, 180)
	root.add_child(bridge)
	check(Fixture.populate(bridge, tex).ok, "Document initialize")
	var before: Image = await capture("before")
	for center in [140, 440]:
		check(near_color(before.get_pixel(center - 40, 140), Fixture.RED), "Top-left UV orientation")
		check(near_color(before.get_pixel(center + 40, 140), Fixture.BLUE), "Top-right UV orientation")
		check(near_color(before.get_pixel(center - 40, 220), Fixture.GREEN), "Bottom-left UV orientation")
		check(near_color(before.get_pixel(center + 40, 220), Fixture.YELLOW), "Bottom-right UV orientation")
	var moved: PackedVector2Array = data.base_positions
	for i in moved.size():
		moved[i] += Vector2(40, 40)
	check(direct.update_positions(moved).ok, "Direct update")
	check(bridge.set_vertex_positions(Fixture.MESH, data.vertex_ids, moved).ok, "Document update")
	var after: Image = await capture("after")
	for center in [140, 440]:
		check(near_color(after.get_pixel(center, 100), Fixture.RED), "Updated vertex buffer: positive Y moves up")
		check(near_color(after.get_pixel(center + 100, 100), Fixture.BLUE), "Bounds expand to new position")
		check(not near_color(after.get_pixel(center - 65, 220), Fixture.GREEN), "Old geometry is gone")
	# Both paths should render identically, before and after updates.
	for y in range(50, 275, 5):
		for x in range(45, 265, 5):
			check(near_color(after.get_pixel(x, y), after.get_pixel(x + 300, y)), "Direct and Document rendering differ")
	direct.free()
	bridge.free()
	await process_frame
	root.content_scale_size = Vector2i(960, 600)
	root.size = Vector2i(960, 600)
	var demo: Node = load("res://main.tscn").instantiate()
	root.add_child(demo)
	await capture("demo")
	demo.free()
	await process_frame
	print("KASANE_RENDER_TEST_", "OK" if failures == 0 else "FAILED", ": ", failures, " failures")
	quit(0 if failures == 0 else 1)
