extends SceneTree
const Fixture = preload("res://fixture.gd")
var failures := 0
var checks := 0
func check(value: bool, message: String) -> void:
	checks += 1
	if not value:
		failures += 1
		push_error(message)
func _initialize() -> void:
	_run.call_deferred()
func _run() -> void:
	var document := KasaneDocumentBridge.new()
	root.add_child(document)
	Fixture.populate(document, Fixture.texture())
	var host = preload("res://script_host.gd").new(document)
	var mesh := document.get_mesh(Fixture.MESH)
	check(mesh.is_valid(), "Live data handle")
	check(host.run_script("res://scripts/edit_document.gd").ok, "Run in-process recipe")
	check(mesh.name == "Script-edited quad", "Property writes reach live Document")
	check(mesh.positions[0] == Vector2(-85, 90), "Script directly edits geometry")
	check(host.actions.history.get_history_count() == 0, "Direct script creates no undo steps")
	check(document.get_mesh_view(Fixture.MESH).get_positions_snapshot() == mesh.positions, "Direct script updates preview")
	var copied := mesh.positions
	copied[0] = Vector2(800, 900)
	check(mesh.positions[0] != copied[0], "Packed arrays require explicit writeback")
	var before := mesh.snapshot()
	check(host.run_script("res://scripts/action_edit.gd").ok, "Explicit action recipe")
	check(host.actions.history is UndoRedo and host.actions.history.get_history_count() == 1, "Native UndoRedo owns history")
	check(host.actions.undo().ok and mesh.positions == before.base_positions and mesh.name == before.name, "Action restores name and positions")
	check(host.actions.redo().ok and mesh.name == "Action-edited quad", "Action redo")
	var expected := mesh.positions
	check(host.actions.perform("Failed action", func():
		mesh.name = "must roll back"
		return {"ok": false, "code": "EXPECTED_FAILURE"}
	).code == "EXPECTED_FAILURE", "Explicit failure propagates")
	check(mesh.name == "Action-edited quad" and mesh.positions == expected, "Failed action restores data")
	check(host.actions.history.get_history_count() == 1, "Failed action adds no history")
	# Native property actions work directly with the same handle, without our action helper.
	var native := UndoRedo.new()
	native.create_action("Property rename")
	native.add_undo_property(mesh, "name", mesh.name)
	native.add_do_property(mesh, "name", "native property")
	native.commit_action()
	check(mesh.name == "native property", "Native property do")
	native.undo()
	check(mesh.name == "Action-edited quad", "Native property undo")
	native.free()
	# Creation and topology changes can be grouped in a single optional action.
	var added_id := "44444444-4444-4444-8444-444444444444"
	check(host.actions.perform("Create mesh", func():
		var data := Fixture.mesh()
		data.id = added_id
		return document.create_mesh(data)
	).ok, "Create action")
	check(document.get_mesh(added_id) != null, "New mesh visible")
	check(host.actions.undo().ok and document.get_mesh(added_id) == null, "Creation undo removes source and view")
	check(document.get_mesh_view(added_id) == null, "Creation undo removes render cache")
	check(host.actions.redo().ok and document.get_mesh(added_id) != null, "Creation redo")
	var topology := mesh.snapshot()
	check(mesh.replace_geometry(PackedInt64Array([1, 2, 3]), PackedVector2Array([Vector2.ZERO, Vector2(20, 0), Vector2(0, 20)]), PackedVector2Array([Vector2.ZERO, Vector2.RIGHT, Vector2.UP]), PackedInt64Array([1, 2, 3])).ok, "Direct topology replacement")
	check(mesh.vertex_ids == PackedInt64Array([1, 2, 3]) and document.get_child_count() == 2, "Topology updates without accumulating views")
	check(not mesh.replace_geometry(PackedInt64Array([1, 2, 3]), mesh.positions, PackedVector2Array([Vector2.ZERO, Vector2.RIGHT, Vector2.UP]), PackedInt64Array([1, 2, 99])).ok, "Invalid topology rejected safely")
	check(host.actions.history.get_history_count() == 2, "Direct topology write does not clear history")
	check(host.run_script("missing.gd").code == "SCRIPT_NOT_FOUND", "Missing script structured error")
	# A failing direct script keeps writes; only explicit actions promise rollback.
	var path := "user://direct-failure.gd"
	var file := FileAccess.open(path, FileAccess.WRITE)
	file.store_string('extends RefCounted\nfunc run(ctx):\n\tctx.document.get_mesh("' + Fixture.MESH + '").name = "retained direct write"\n\treturn {"ok": false, "code": "EXPECTED_FAILURE"}\n')
	file.close()
	check(host.run_script(path).code == "EXPECTED_FAILURE" and mesh.name == "retained direct write", "No implicit transaction around scripts")
	var old_state := document.capture_state()
	var old_revision: int = document.get_document_summary().revision
	check(document.open_project("res://samples/stage-03.kasane.json").ok, "Reopen live document")
	check(document.get_document_summary().revision > old_revision, "Reopen does not reuse revision")
	check(not mesh.is_valid(), "Reopen invalidates old data handles")
	check(mesh.set_vertex_positions(PackedInt64Array([40]), PackedVector2Array([Vector2.ZERO])).code == "STALE_HANDLE", "Old handle cannot edit reopened data")
	check(document.restore_state(old_state).code == "STALE_STATE", "Old action state cannot overwrite reopened document")
	check(not host.actions.undo().ok and host.actions.history.get_history_count() == 0, "Action history resets across document sessions")
	mesh = document.get_mesh(Fixture.MESH)
	document.free()
	check(not mesh.is_valid(), "Handle safe after bridge deletion")
	print("KASANE_SCRIPT_TEST_", "OK" if failures == 0 else "FAILED", ": ", checks, " checks, ", failures, " failures")
	quit(0 if failures == 0 else 1)
