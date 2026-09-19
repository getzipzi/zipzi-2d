extends SceneTree
const Fixture = preload("res://fixture.gd")
const ROT := "44444444-4444-4444-8444-444444444444"
const WARP := "55555555-5555-4555-8555-555555555555"
const OTHER := "66666666-6666-4666-8666-666666666666"
var checks := 0
var failures := 0
func check(value: bool, message: String) -> void:
	checks += 1
	if not value:
		failures += 1
		push_error(message)
func _initialize() -> void:
	_run.call_deferred()
func write_json(path: String, value: Dictionary) -> void:
	var file := FileAccess.open(path, FileAccess.WRITE)
	file.store_string(JSON.stringify(value))
	file.close()
func _run() -> void:
	var doc := KasaneDocumentBridge.new()
	root.add_child(doc)
	check(Fixture.populate(doc, Fixture.texture()).ok, "Initialize")
	var other := Fixture.mesh()
	other.id = OTHER
	check(doc.create_mesh(other).ok, "Independent mesh")
	var mesh := doc.get_mesh(Fixture.MESH)
	var source := PackedVector2Array([Vector2.ZERO, Vector2(10, 0), Vector2(10, 10), Vector2(5, 5)])
	check(mesh.replace_geometry(PackedInt64Array([0, 1, 2, 3]), source, PackedVector2Array([Vector2.ZERO, Vector2.RIGHT, Vector2.ONE, Vector2(0.5, 0.5)]), PackedInt64Array([0, 1, 2, 0, 2, 3])).ok, "Known core fixture")
	var untouched := doc.get_mesh_view(OTHER).get_render_stats()
	var mesh_rid: int = doc.get_mesh_view(Fixture.MESH).get_render_stats().mesh_rid
	check(doc.create_rotation(ROT, "rotation", Vector2.ZERO, 90.0).ok, "Create rotation")
	check(doc.set_deform_parent(Fixture.MESH, ROT).ok, "Bind mesh")
	check(doc.evaluate_mesh(Fixture.MESH).positions[1].is_equal_approx(Vector2(0, 10)), "Matches core 90 degree result")
	check(doc.get_mesh_view(Fixture.MESH).get_positions_snapshot() == doc.evaluate_mesh(Fixture.MESH).positions, "Preview uses evaluated data")
	check(mesh.positions == source, "Rotation leaves source unchanged")
	var rotation := doc.get_deformer(ROT)
	rotation.center = Vector2(5, 5)
	rotation.angle_degrees = 180.0
	check(doc.evaluate_mesh(Fixture.MESH).positions[0].is_equal_approx(Vector2(10, 10)), "Rotation center")
	check(rotation.update_rotation(Vector2.ZERO, 90.0).ok, "Reset rotation")
	check(doc.create_warp(WARP, "warp", Vector2.ZERO, Vector2(10, 10), 1, 1).ok, "Create Warp")
	var warp := doc.get_deformer(WARP)
	check(doc.set_deform_parent(Fixture.MESH, WARP).ok, "Bind Warp")
	check(doc.evaluate_mesh(Fixture.MESH).positions == source, "Identity Warp")
	var points := warp.control_points
	points[3] = Vector2(20, 20)
	warp.control_points = points
	check(doc.evaluate_mesh(Fixture.MESH).positions[3].is_equal_approx(Vector2(7.5, 7.5)), "Bilinear midpoint matches core")
	check(warp.bind_to(ROT).ok, "Nest Warp under Rotation")
	check(doc.evaluate_mesh(Fixture.MESH).positions[3].is_equal_approx(Vector2(-7.5, 7.5)), "Child then parent order matches core")
	check(doc.get_mesh_view(OTHER).get_render_stats() == untouched, "Independent mesh never uploads")
	check(doc.get_mesh_view(Fixture.MESH).get_render_stats().mesh_rid == mesh_rid, "Deformation reuses mesh RID")
	check(doc.set_organization_parent(OTHER, ROT).ok, "Separate organization link")
	check(doc.get_mesh_view(OTHER).get_render_stats() == untouched, "Organization does not deform or upload")
	var before := doc.get_mesh_view(Fixture.MESH).get_positions_snapshot()
	var revision: int = doc.get_document_summary().revision
	check(rotation.bind_to(WARP).code == "PARENT_CYCLE", "Reject deformation cycle")
	check(doc.set_organization_parent(ROT, OTHER).code == "PARENT_CYCLE", "Reject organization cycle")
	check(doc.set_deform_parent(ROT, OTHER).code == "INVALID_PARENT", "Reject mesh as deformation parent")
	check(doc.get_document_summary().revision == revision, "Failed links do not mutate document")
	check(not warp.update_control_points(PackedVector2Array([Vector2.ZERO])).ok, "Reject wrong control count")
	var bad_points := points.duplicate()
	bad_points[0].x = NAN
	check(not warp.update_control_points(bad_points).ok, "Reject nonfinite controls")
	check(not rotation.update_rotation(Vector2.ZERO, INF).ok, "Reject nonfinite angle")
	check(doc.get_mesh_view(Fixture.MESH).get_positions_snapshot() == before, "Invalid data leaves preview intact")
	var actions = preload("res://actions.gd").new(doc)
	check(doc.save_project("user://stage05.kasane.json").ok, "Save v2")
	check(actions.perform("Shape and rotate", func():
		var p := warp.control_points
		p[0] += Vector2(2, 3)
		var r := warp.update_control_points(p)
		if not r.ok: return r
		return rotation.update_rotation(Vector2.ZERO, 25)
	).ok, "One explicit action")
	var changed: PackedVector2Array = doc.evaluate_mesh(Fixture.MESH).positions
	check(actions.history.get_history_count() == 1, "One history step")
	check(actions.undo().ok, "Undo")
	check(doc.evaluate_mesh(Fixture.MESH).positions == before and not doc.get_document_summary().modified, "Undo restores deformed result and save point")
	check(actions.redo().ok and doc.evaluate_mesh(Fixture.MESH).positions == changed, "Redo")
	check(mesh.positions == source, "Actions never overwrite source vertices")
	check(doc.save_project("user://stage05.kasane.json").ok, "Save edited v2")
	var saved: Dictionary = JSON.parse_string(FileAccess.get_file_as_string("user://stage05.kasane.json"))
	check(saved.format_version == 2, "Version bump")
	var reopened := KasaneDocumentBridge.new()
	root.add_child(reopened)
	check(reopened.open_project("user://stage05.kasane.json").ok, "Reopen")
	check(reopened.evaluate_mesh(Fixture.MESH).positions == changed, "Reopened evaluation matches")
	check(reopened.get_mesh(Fixture.MESH).positions == source, "Persist source, not evaluated vertices")
	check(reopened.get_deformer(WARP).control_points == warp.control_points, "Persist control points")
	check(reopened.get_mesh_snapshot(OTHER).organization_parent == ROT and reopened.get_mesh_snapshot(OTHER).deform_parent == "", "Persist separate relationships")
	var broken := saved.duplicate(true)
	broken.document.deformation_links.append({"child": ROT, "parent": WARP})
	write_json("user://stage05-invalid.json", broken)
	check(reopened.open_project("user://stage05-invalid.json").code == "PARENT_CYCLE", "Reject saved cycle")
	check(reopened.evaluate_mesh(Fixture.MESH).positions == changed, "Failed open preserves live model")
	broken = saved.duplicate(true)
	broken.document.deformers[1].control_points = []
	write_json("user://stage05-invalid.json", broken)
	check(reopened.open_project("user://stage05-invalid.json").code == "INVALID_LENGTH", "Reject missing saved controls")
	check(reopened.evaluate_mesh(Fixture.MESH).positions == changed, "Malformed Warp preserves model")
	check(doc.open_project("user://stage05.kasane.json").ok, "Reopen same bridge")
	check(not rotation.is_valid() and rotation.update_rotation(Vector2.ZERO, 0).code == "STALE_HANDLE", "Old deformer handle invalidated")
	check(not actions.undo().ok, "Old action cannot restore prior session")
	reopened.free()
	doc.free()
	# Run the delivered subdivided example as a single optional action.
	var sample := KasaneDocumentBridge.new()
	root.add_child(sample)
	Fixture.populate(sample, Fixture.texture())
	var host = preload("res://script_host.gd").new(sample)
	check(host.actions.perform("Build deformer sample", func(): return host.run_script("res://scripts/stage05_deformers.gd")).ok, "Example script")
	check(sample.get_document_summary().deformers.size() == 2, "Example creates both deformers")
	check(sample.get_mesh(Fixture.MESH).positions.size() == 25, "Example has subdivided mesh")
	check(host.actions.undo().ok and sample.get_document_summary().deformers.is_empty(), "Undo creation and bindings")
	check(host.actions.redo().ok and sample.get_document_summary().deformers.size() == 2, "Redo creation and bindings")
	var sample_evaluated: PackedVector2Array = sample.evaluate_mesh(Fixture.MESH).positions
	check(sample.open_project("res://samples/stage-05.kasane.json").ok, "Checked-in Stage 05 sample opens")
	check(sample.evaluate_mesh(Fixture.MESH).positions == sample_evaluated, "Checked-in sample matches delivered recipe")
	sample.free()
	print("KASANE_DEFORMER_TEST_", "OK" if failures == 0 else "FAILED", ": ", checks, " checks, ", failures, " failures")
	quit(0 if failures == 0 else 1)
