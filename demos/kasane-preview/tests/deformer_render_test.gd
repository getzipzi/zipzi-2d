extends SceneTree
const Fixture = preload("res://fixture.gd")
const ROT := "44444444-4444-4444-8444-444444444444"
const WARP := "55555555-5555-4555-8555-555555555555"
var failures := 0
func check(value: bool, message: String) -> void:
	if not value:
		failures += 1
		push_error(message)
func near_color(a: Color, b: Color) -> bool:
	return abs(a.r-b.r) < 0.06 and abs(a.g-b.g) < 0.06 and abs(a.b-b.b) < 0.06
func capture(path: String) -> Image:
	await process_frame
	await RenderingServer.frame_post_draw
	var image := root.get_texture().get_image()
	check(image != null and not image.is_empty(), "Actual renderer required")
	DirAccess.make_dir_recursive_absolute(ProjectSettings.globalize_path("res://artifacts"))
	check(image.save_png("res://artifacts/" + path + ".png") == OK, "Save GPU evidence")
	return image
func _initialize() -> void:
	_run.call_deferred()
func _run() -> void:
	root.size = Vector2i(640, 360)
	root.content_scale_size = Vector2i(640, 360)
	var doc := KasaneDocumentBridge.new()
	doc.position = Vector2(320, 180)
	root.add_child(doc)
	check(Fixture.populate(doc, Fixture.texture()).ok, "Populate")
	var original := doc.get_mesh(Fixture.MESH).positions
	check(doc.create_rotation(ROT, "rotation", Vector2.ZERO, 90).ok, "Create rotation")
	check(doc.set_deform_parent(Fixture.MESH, ROT).ok, "Bind rotation")
	var rotated: Image = await capture("stage05-rotation")
	check(near_color(rotated.get_pixel(280, 220), Fixture.RED), "90 degree rotation moves red to bottom left")
	check(near_color(rotated.get_pixel(280, 140), Fixture.BLUE), "Blue to top left")
	check(near_color(rotated.get_pixel(360, 220), Fixture.GREEN), "Green to bottom right")
	check(near_color(rotated.get_pixel(360, 140), Fixture.YELLOW), "Yellow to top right")
	check(doc.create_warp(WARP, "warp", Vector2(-80, -80), Vector2(160, 160), 1, 1).ok, "Create Warp")
	check(doc.set_deform_parent(Fixture.MESH, WARP).ok, "Bind Warp")
	check(doc.get_deformer(WARP).bind_to(ROT).ok, "Nest")
	var points := doc.get_deformer(WARP).control_points
	for i in points.size(): points[i] += Vector2(40, 0)
	check(doc.get_deformer(WARP).update_control_points(points).ok, "Warp translation")
	var warped: Image = await capture("stage05-warp-rotation")
	check(near_color(warped.get_pixel(280, 180), Fixture.RED), "Warp before rotation moves red upward")
	check(near_color(warped.get_pixel(280, 100), Fixture.BLUE), "Nested deformation reaches GPU")
	check(not near_color(warped.get_pixel(280, 250), Fixture.RED), "Old geometry disappears")
	check(doc.get_mesh(Fixture.MESH).positions == original, "Rendering leaves source unchanged")
	check(doc.save_project("user://stage05-render.json").ok, "Save rendered model")
	check(doc.open_project("user://stage05-render.json").ok, "Reopen rendered model")
	var reopened: Image = await capture("stage05-reopened")
	for y in range(65, 215, 7):
		for x in range(245, 395, 7):
			check(near_color(warped.get_pixel(x,y), reopened.get_pixel(x,y)), "Save/reopen renders identically")
	var host = preload("res://script_host.gd").new(doc)
	check(doc.set_rotation(ROT, Vector2.ZERO, 45).ok, "Prepare overflow check")
	var extreme := PackedVector2Array([Vector2(3.0e38, 3.0e38), Vector2(3.0e38, 3.0e38), Vector2(3.0e38, 3.0e38), Vector2(3.0e38, 3.0e38)])
	var overflow := doc.set_warp_points(WARP, extreme)
	check(overflow.ok and not overflow.preview_ok, "Separate source write from evaluation failure")
	var rejected: Dictionary = await host.capture(root)
	check(not rejected.ok and rejected.code == "EVALUATION_OVERFLOW", "Never label stale pixels as current deformed state")
	doc.free()
	await process_frame
	root.size = Vector2i(960, 600)
	root.content_scale_size = Vector2i(960, 600)
	var demo: Node = load("res://main.tscn").instantiate()
	root.add_child(demo)
	demo.script_path.text = "res://scripts/stage05_deformers.gd"
	check(demo.script_host.run_script(demo.script_path.text).ok, "Run delivered sample")
	await capture("stage05-demo")
	demo.free()
	await process_frame
	print("KASANE_DEFORMER_RENDER_TEST_", "OK" if failures == 0 else "FAILED", ": ", failures, " failures")
	quit(0 if failures == 0 else 1)
