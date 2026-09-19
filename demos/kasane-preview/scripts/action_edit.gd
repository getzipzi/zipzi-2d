extends RefCounted

# The same data API, explicitly grouped into one native UndoRedo action.
func run(ctx) -> Dictionary:
	return ctx.actions.perform("Shape upper edge", func():
		var mesh: KasaneMeshData = ctx.document.get_mesh(ctx.document.get_document_summary().meshes[0].id)
		mesh.name = "Action-edited quad"
		var positions := mesh.positions
		positions[0] += Vector2(-10, 15)
		positions[3] += Vector2(10, 15)
		mesh.positions = positions
		return {"ok": true}
	)
