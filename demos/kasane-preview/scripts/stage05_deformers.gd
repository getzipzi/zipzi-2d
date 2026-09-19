extends RefCounted
const ROTATION := "44444444-4444-4444-8444-444444444444"
const WARP := "55555555-5555-4555-8555-555555555555"

func run(ctx) -> Dictionary:
	var doc: KasaneDocumentBridge = ctx.document
	var mesh: KasaneMeshData = doc.get_mesh(doc.get_document_summary().meshes[0].id)
	# A subdivided source mesh makes interior Warp controls visible.
	var ids := PackedInt64Array()
	var positions := PackedVector2Array()
	var uvs := PackedVector2Array()
	var triangles := PackedInt64Array()
	for y in 5:
		for x in 5:
			ids.append(y * 5 + x)
			positions.append(Vector2(-80 + x * 40, -80 + y * 40))
			uvs.append(Vector2(x / 4.0, 1.0 - y / 4.0))
	for y in 4:
		for x in 4:
			var a := y * 5 + x
			triangles.append_array(PackedInt64Array([a, a + 1, a + 6, a, a + 6, a + 5]))
	var result := mesh.replace_geometry(ids, positions, uvs, triangles)
	if not result.ok:
		return result
	if doc.get_deformer(ROTATION) == null:
		result = doc.create_rotation(ROTATION, "Part rotation", Vector2.ZERO, 0.0)
		if not result.ok:
			return result
	if doc.get_deformer(WARP) == null:
		result = doc.create_warp(WARP, "Part warp", Vector2(-80, -80), Vector2(160, 160), 2, 2)
		if not result.ok:
			return result
	result = doc.set_deform_parent(mesh.id, WARP)
	if not result.ok:
		return result
	result = doc.get_deformer(WARP).bind_to(ROTATION)
	if not result.ok:
		return result
	result = doc.set_organization_parent(mesh.id, ROTATION)
	if not result.ok:
		return result
	var points := PackedVector2Array()
	for y in 3:
		for x in 3:
			points.append(Vector2(-80 + x * 80, -80 + y * 80))
	points[4] += Vector2(20, 0)
	points[7] += Vector2(0, 35)
	# Direct data edits; callers may explicitly wrap the entire recipe in an Action.
	result = doc.get_deformer(WARP).update_control_points(points)
	if not result.ok:
		return result
	result = doc.get_deformer(ROTATION).update_rotation(Vector2.ZERO, 20.0)
	if not result.ok:
		return result
	return doc.evaluate_mesh(mesh.id)
