extends RefCounted

# Direct edits: no transaction and no automatically generated history.
func run(ctx) -> Dictionary:
	var summary: Dictionary = ctx.document.get_document_summary()
	var mesh: KasaneMeshData = ctx.document.get_mesh(summary.meshes[0].id)
	mesh.name = "Script-edited quad"
	var positions := mesh.positions
	positions[0] += Vector2(-5, 10)
	mesh.positions = positions
	return {"ok": true, "mesh": mesh.snapshot(), "history_steps": ctx.actions.history.get_history_count()}
