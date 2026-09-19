extends SceneTree
const Fixture = preload("res://fixture.gd")
func _initialize() -> void:
	_run.call_deferred()
func _run() -> void:
	var document := KasaneDocumentBridge.new()
	root.add_child(document)
	Fixture.populate(document, Fixture.texture())
	var host = preload("res://script_host.gd").new(document)
	var path := "user://runtime-error.gd"
	var file := FileAccess.open(path, FileAccess.WRITE)
	file.store_string('extends RefCounted\nfunc run(ctx):\n\tctx.document.get_mesh("' + Fixture.MESH + '").name = "written before error"\n\tvar absent: Variant = null\n\tabsent.deliberate_runtime_error()\n\treturn {"ok": true}\n')
	file.close()
	var outcome: Dictionary = host.run_script(path)
	var mesh := document.get_mesh(Fixture.MESH)
	var direct_ok: bool = not outcome.ok and outcome.code == "SCRIPT_NO_RESULT" and mesh.name == "written before error"
	mesh.name = "before action"
	outcome = host.actions.perform("Runtime failure", func(): return host.run_script(path))
	var action_ok: bool = not outcome.ok and mesh.name == "before action" and not host.actions.history.has_undo()
	document.free()
	var success := direct_ok and action_ok
	print("KASANE_SCRIPT_ERROR_TEST_", "OK" if success else "FAILED")
	quit(0 if success else 1)
