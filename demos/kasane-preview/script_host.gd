extends RefCounted
## Trusted, synchronous scripts run in this Godot process against the live model.
var document: KasaneDocumentBridge
var actions: RefCounted
var last_result: Dictionary = {}
var _running := false

func _init(current_document: KasaneDocumentBridge) -> void:
	document = current_document
	actions = preload("res://actions.gd").new(document)

func run_script(path: String) -> Dictionary:
	if _running:
		return {"ok": false, "code": "SCRIPT_RUNNING"}
	if not FileAccess.file_exists(path):
		return {"ok": false, "code": "SCRIPT_NOT_FOUND", "path": path}
	# Fresh source on each execution; editing a recipe never reloads the model.
	var script := GDScript.new()
	script.source_code = FileAccess.get_file_as_string(path)
	script.take_over_path(path)
	if script.reload() != OK or not script.can_instantiate() or script.get_instance_base_type() != &"RefCounted":
		return {"ok": false, "code": "INVALID_SCRIPT", "path": path}
	var instance = script.new()
	if not instance.has_method("run"):
		return {"ok": false, "code": "MISSING_RUN"}
	_running = true
	var outcome = instance.run(self)
	_running = false
	last_result = outcome if outcome is Dictionary else {"ok": false, "code": "SCRIPT_NO_RESULT"}
	last_result["revision"] = document.get_document_summary().revision
	return last_result

func capture(viewport: Viewport) -> Dictionary:
	if DisplayServer.get_name() == "headless":
		return {"ok": false, "code": "RENDERER_REQUIRED"}
	var revision: int = document.get_document_summary().revision
	await RenderingServer.frame_post_draw
	if document.get_document_summary().revision != revision:
		return {"ok": false, "code": "PREVIEW_CHANGED"}
	var image := viewport.get_texture().get_image()
	if image == null or image.is_empty():
		return {"ok": false, "code": "CAPTURE_FAILED"}
	return {"ok": true, "revision": revision, "image": image}
