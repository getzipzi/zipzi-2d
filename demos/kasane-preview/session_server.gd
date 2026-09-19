extends Node

# One loopback client at a time. Draft commands live here, never in Document.
const MAX_LINE := 1048576
var bridge: KasaneDocumentBridge
var listener: TCPServer
var peer: StreamPeerTCP
var pending := ""
var base_revision := -1
var staged: Array[Dictionary] = []

func start(document_bridge: KasaneDocumentBridge, port: int = 43884) -> Dictionary:
	bridge = document_bridge
	listener = TCPServer.new()
	var status := listener.listen(port, "127.0.0.1")
	if status != OK:
		return _error("LISTEN_FAILED", "Could not bind the local editing port")
	return {"ok": true, "host": "127.0.0.1", "port": port}

func _exit_tree() -> void:
	_drop_client()
	if listener != null:
		listener.stop()

func _process(_delta: float) -> void:
	if listener == null:
		return
	if peer == null and listener.is_connection_available():
		peer = listener.take_connection()
		pending = ""
	if peer == null:
		return
	peer.poll()
	if peer.get_status() != StreamPeerTCP.STATUS_CONNECTED:
		_drop_client()
		return
	var available := peer.get_available_bytes()
	if available <= 0:
		return
	var read: Array = peer.get_data(mini(available, MAX_LINE + 1))
	if read[0] != OK:
		_drop_client()
		return
	pending += (read[1] as PackedByteArray).get_string_from_utf8()
	if pending.length() > MAX_LINE:
		_send(_error("REQUEST_TOO_LARGE", "Request exceeds one MiB"))
		_drop_client()
		return
	while peer != null and pending.contains("\n"):
		var split := pending.find("\n")
		var line := pending.substr(0, split)
		pending = pending.substr(split + 1)
		var request = JSON.parse_string(line)
		if typeof(request) != TYPE_DICTIONARY:
			_send(_error("INVALID_REQUEST", "Expected one JSON object per line"))
		else:
			_send(_dispatch(request))

func _drop_client() -> void:
	if peer != null:
		peer.disconnect_from_host()
	peer = null
	pending = ""
	base_revision = -1
	staged.clear()

func _send(response: Dictionary) -> void:
	if peer != null:
		peer.put_data((JSON.stringify(response) + "\n").to_utf8_buffer())

func _error(code: String, message: String) -> Dictionary:
	return {"ok": false, "code": code, "message": message}

func _revision() -> int:
	return bridge.get_document_summary().revision

func _pairs(values: PackedVector2Array) -> Array:
	var out := []
	for value in values:
		out.append([value.x, value.y])
	return out

func _mesh_result(mesh_id: String) -> Dictionary:
	var snapshot := bridge.get_mesh_snapshot(mesh_id)
	if not snapshot.ok:
		return snapshot
	snapshot["vertex_ids"] = Array(snapshot.vertex_ids)
	snapshot["triangles"] = Array(snapshot.triangles)
	snapshot["base_positions"] = _pairs(snapshot.base_positions)
	snapshot["uvs"] = _pairs(snapshot.uvs)
	return snapshot

func _update(request: Dictionary) -> Dictionary:
	if not request.has("mesh_id") or typeof(request.mesh_id) != TYPE_STRING:
		return _error("INVALID_FIELD", "mesh_id must be a string")
	if not request.has("vertex_ids") or typeof(request.vertex_ids) != TYPE_ARRAY:
		return _error("INVALID_FIELD", "vertex_ids must be an array")
	if not request.has("positions") or typeof(request.positions) != TYPE_ARRAY:
		return _error("INVALID_FIELD", "positions must be an array")
	var ids := PackedInt64Array()
	var positions := PackedVector2Array()
	for id in request.vertex_ids:
		if typeof(id) != TYPE_FLOAT or id < 0 or id > 4294967295 or floor(id) != id:
			return _error("INVALID_VERTEX_ID", "vertex_ids must contain uint32 integers")
		ids.append(int(id))
	for pair in request.positions:
		if typeof(pair) != TYPE_ARRAY or pair.size() != 2 or typeof(pair[0]) != TYPE_FLOAT or typeof(pair[1]) != TYPE_FLOAT:
			return _error("INVALID_FIELD", "positions must contain [x, y] number pairs")
		positions.append(Vector2(pair[0], pair[1]))
	return {"ok": true, "mesh_id": request.mesh_id, "vertex_ids": ids, "positions": positions}

func _grid(request: Dictionary) -> Dictionary:
	for key in ["id", "name", "texture_asset_id"]:
		if not request.has(key) or typeof(request[key]) != TYPE_STRING:
			return _error("INVALID_FIELD", key + " must be a string")
	for key in ["columns", "rows", "width", "height"]:
		if not request.has(key) or typeof(request[key]) != TYPE_FLOAT:
			return _error("INVALID_FIELD", key + " must be a number")
	if not is_finite(request.columns) or not is_finite(request.rows) or request.columns < 1 or request.rows < 1 or request.columns > 4095 or request.rows > 4095:
		return _error("INVALID_GRID", "Grid cell counts must be finite and between 1 and 4095")
	var columns := int(request.columns)
	var rows := int(request.rows)
	if columns < 1 or rows < 1 or (columns + 1) * (rows + 1) > 4096 or columns != request.columns or rows != request.rows:
		return _error("INVALID_GRID", "Grid needs positive integer cells and at most 4096 vertices")
	if not is_finite(request.width) or not is_finite(request.height) or request.width <= 0 or request.height <= 0:
		return _error("INVALID_GRID", "Grid dimensions must be finite and positive")
	var ids := PackedInt64Array()
	var positions := PackedVector2Array()
	var uvs := PackedVector2Array()
	var triangles := PackedInt64Array()
	for y in rows + 1:
		for x in columns + 1:
			ids.append(y * (columns + 1) + x)
			positions.append(Vector2((float(x) / columns - 0.5) * request.width, (0.5 - float(y) / rows) * request.height))
			uvs.append(Vector2(float(x) / columns, float(y) / rows))
	for y in rows:
		for x in columns:
			var a := y * (columns + 1) + x
			var b := a + 1
			var c := a + columns + 1
			var d := c + 1
			triangles.append_array(PackedInt64Array([a, c, d, a, d, b]))
	return bridge.create_mesh({"id": request.id, "name": request.name, "texture_asset_id": request.texture_asset_id,
		"vertex_ids": ids, "base_positions": positions, "uvs": uvs, "triangles": triangles})

func _dispatch(request: Dictionary) -> Dictionary:
	if not request.has("method") or typeof(request.method) != TYPE_STRING:
		return _error("INVALID_REQUEST", "method must be a string")
	match request.method:
		"summary":
			var summary := bridge.get_document_summary()
			summary["ok"] = true
			summary["canvas_size"] = [summary.canvas_size.x, summary.canvas_size.y]
			return summary
		"get_mesh":
			if not request.has("id") or typeof(request.id) != TYPE_STRING:
				return _error("INVALID_FIELD", "id must be a string")
			return _mesh_result(request.id)
		"get_asset":
			if not request.has("id") or typeof(request.id) != TYPE_STRING:
				return _error("INVALID_FIELD", "id must be a string")
			return bridge.get_asset_snapshot(request.id)
		"preview":
			if not request.has("id") or typeof(request.id) != TYPE_STRING:
				return _error("INVALID_FIELD", "id must be a string")
			var view := bridge.get_mesh_view(request.id)
			if view == null:
				return _error("MISSING_MESH", "Preview mesh does not exist")
			return {"ok": true, "positions": _pairs(view.get_positions_snapshot()), "revision": _revision()}
		"begin":
			if base_revision >= 0:
				return _error("TRANSACTION_ACTIVE", "This connection already has a transaction")
			if not request.has("revision") or typeof(request.revision) != TYPE_FLOAT or request.revision < 0 or floor(request.revision) != request.revision:
				return _error("INVALID_REVISION", "revision must be a nonnegative integer")
			base_revision = int(request.revision)
			staged.clear()
			return {"ok": true, "revision": base_revision}
		"stage_positions":
			if base_revision < 0:
				return _error("NO_TRANSACTION", "Begin a transaction first")
			var update := _update(request)
			if not update.ok:
				return update
			staged.append(update)
			return {"ok": true, "staged_count": staged.size()}
		"commit":
			if base_revision < 0:
				return _error("NO_TRANSACTION", "Begin a transaction first")
			var expected := base_revision
			var batch := staged.duplicate()
			base_revision = -1
			staged.clear()
			return bridge.commit_vertex_updates(batch, expected)
		"cancel":
			if base_revision < 0:
				return _error("NO_TRANSACTION", "No transaction is active")
			base_revision = -1
			staged.clear()
			return {"ok": true}
		"create_grid":
			if base_revision >= 0:
				return _error("TRANSACTION_ACTIVE", "Commit or cancel the draft before creating a mesh")
			if not request.has("revision") or typeof(request.revision) != TYPE_FLOAT or request.revision < 0 or floor(request.revision) != request.revision or int(request.revision) != _revision():
				return _error("STALE_REVISION", "Document changed before grid creation")
			return _grid(request)
		"undo":
			if base_revision >= 0:
				return _error("TRANSACTION_ACTIVE", "Commit or cancel the draft first")
			return bridge.undo()
		"redo":
			if base_revision >= 0:
				return _error("TRANSACTION_ACTIVE", "Commit or cancel the draft first")
			return bridge.redo()
	return _error("UNKNOWN_METHOD", "Unknown editing method")
