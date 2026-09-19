extends RefCounted
## Optional whole-document actions. Direct data edits do not use this service.
var history := UndoRedo.new()
var _document: WeakRef
var _generation: int
var _active := false

func _init(document: KasaneDocumentBridge) -> void:
	_document = weakref(document)
	_generation = document.get_document_summary().generation
	history.max_steps = 128

func _sync() -> KasaneDocumentBridge:
	var document = _document.get_ref()
	if document == null:
		history.clear_history()
		return null
	var generation: int = document.get_document_summary().generation
	if generation != _generation:
		history.clear_history()
		_generation = generation
	return document

func perform(label: String, operation: Callable) -> Dictionary:
	var document := _sync()
	if document == null:
		return {"ok": false, "code": "CLOSED_DOCUMENT"}
	if _active or document.get_document_summary().transaction_active:
		return {"ok": false, "code": "ACTION_ACTIVE"}
	var before := document.capture_state()
	_active = true
	var outcome = operation.call()
	_active = false
	if document.get_document_summary().generation != _generation:
		_sync()
		return {"ok": false, "code": "DOCUMENT_REPLACED"}
	# Only explicit success commits an action. This is not a script sandbox:
	# file writes and other external side effects cannot be rolled back here.
	if document.get_document_summary().transaction_active:
		document.cancel_transaction()
		outcome = {"ok": false, "code": "UNFINISHED_TRANSACTION"}
	if not outcome is Dictionary or not outcome.get("ok", false):
		document.restore_state(before)
		return outcome if outcome is Dictionary else {"ok": false, "code": "ACTION_FAILED"}
	var after := document.capture_state()
	history.create_action(label)
	history.add_undo_method(document.restore_state.bind(before))
	history.add_do_method(document.restore_state.bind(after))
	history.commit_action(false)
	return outcome

func undo() -> Dictionary:
	var document := _sync()
	if document == null:
		return {"ok": false, "code": "CLOSED_DOCUMENT"}
	if _active or document.get_document_summary().transaction_active:
		return {"ok": false, "code": "ACTION_ACTIVE"}
	var succeeded := history.undo()
	return {"ok": succeeded, "code": "" if succeeded else "NOTHING_TO_UNDO"}

func redo() -> Dictionary:
	var document := _sync()
	if document == null:
		return {"ok": false, "code": "CLOSED_DOCUMENT"}
	if _active or document.get_document_summary().transaction_active:
		return {"ok": false, "code": "ACTION_ACTIVE"}
	var succeeded := history.redo()
	return {"ok": succeeded, "code": "" if succeeded else "NOTHING_TO_REDO"}

func _notification(what: int) -> void:
	if what == NOTIFICATION_PREDELETE and is_instance_valid(history):
		history.free()
