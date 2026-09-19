extends RefCounted
func run(ctx) -> Dictionary:
	var mesh: KasaneMeshData = ctx.document.get_mesh(ctx.document.get_document_summary().meshes[0].id)
	var positions := mesh.positions
	for i in positions.size():
		positions[i] += Vector2(0, 40)
	mesh.positions = positions
	return {"ok": true}
