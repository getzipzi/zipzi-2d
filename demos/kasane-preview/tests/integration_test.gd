extends SceneTree

const Fixture = preload("res://fixture.gd")
var failures := 0
var checks := 0

func check(condition: bool, message: String) -> void:
	checks += 1
	if not condition:
		failures += 1
		push_error(message)

func _initialize() -> void:
	_run.call_deferred()

func _run() -> void:
	var tex := Fixture.texture()
	var texture_refs := tex.get_reference_count()
	var data := Fixture.mesh()
	var direct := KasaneMeshView.new()
	root.add_child(direct)
	check(direct.initialize(data.base_positions, data.uvs, PackedInt32Array([0, 1, 2, 0, 2, 3]), tex).ok, "Direct initialize")
	var before := direct.get_render_stats()
	var positions: PackedVector2Array = data.base_positions
	positions[0] = Vector2(-100, 100)
	check(direct.get_positions_snapshot()[0] == Vector2(-80, 80), "Input must be copied")
	for i in 40:
		positions[0].x = -100 - i
		check(direct.update_positions(positions).ok, "Direct position update")
	check(direct.get_render_stats().mesh_rid == before.mesh_rid, "Update must reuse mesh RID")
	check(direct.get_render_stats().texture_instance_id == before.texture_instance_id, "Update must reuse texture")
	check(direct.get_render_stats().surface_creations == 1, "Only one surface creation")
	var valid := direct.get_positions_snapshot()
	check(not direct.update_positions(PackedVector2Array([Vector2.ZERO])).ok, "Reject wrong length")
	positions[0].x = NAN
	check(not direct.update_positions(positions).ok, "Reject NaN")
	check(direct.get_positions_snapshot() == valid, "Failed update leaves previous data")
	data = Fixture.mesh()
	check(not direct.initialize(data.base_positions, data.uvs, PackedInt32Array([0, 1, 99]), tex).ok, "Reject invalid topology")
	check(direct.get_render_stats().mesh_rid == before.mesh_rid, "Failed initialize preserves resources")
	direct.clear()
	direct.clear()
	check(direct.mesh == null and direct.texture == null, "Clear releases owned resources")
	check(not direct.update_positions(valid).ok, "Update after clear must fail")
	for i in 40:
		check(direct.initialize(data.base_positions, data.uvs, PackedInt32Array([0, 1, 2, 0, 2, 3]), tex).ok, "Reinitialize")
		direct.clear()
	check(direct.get_child_count() == 0, "No accumulating mesh children")
	direct.free()
	check(tex.get_reference_count() == texture_refs, "Create/clear cycles release texture references")

	var bridge := KasaneDocumentBridge.new()
	root.add_child(bridge)
	check(Fixture.populate(bridge, tex).ok, "Populate document")
	var snapshot := bridge.get_mesh_snapshot(Fixture.MESH)
	var changed_copy: PackedVector2Array = snapshot.base_positions
	changed_copy[0] = Vector2(999, 999)
	check(bridge.get_mesh_snapshot(Fixture.MESH).base_positions[0] == Vector2(-80, 80), "Snapshots must not expose mutable source")
	var view := bridge.get_mesh_view(Fixture.MESH)
	var render_before := view.get_render_stats()
	check(bridge.rename_mesh(Fixture.MESH, "renamed").change_kind == "metadata", "Metadata changes")
	check(view.get_render_stats() == render_before, "Rename must not upload or rebuild mesh")
	var revision: int = bridge.get_document_summary().revision
	check(not bridge.set_vertex_positions(Fixture.MESH, PackedInt64Array([40, 999]), PackedVector2Array([Vector2.ZERO, Vector2.ONE])).ok, "Reject missing vertex in batch")
	check(bridge.get_document_summary().revision == revision, "Failed edit does not advance revision")
	check(view.get_positions_snapshot() == Fixture.mesh().base_positions, "Failed edit does not update renderer")
	var edit := bridge.set_vertex_positions(Fixture.MESH, PackedInt64Array([20]), PackedVector2Array([Vector2(120, 130)]))
	check(edit.ok and edit.change_kind == "positions", "Stable-ID edit")
	check(view.get_positions_snapshot()[3] == Vector2(120, 130), "Vertex ID resolves to dense slot")
	check(view.get_render_stats().mesh_rid == render_before.mesh_rid, "Document update reuses renderer")
	check(view.mesh.get_custom_aabb().end.x == 120 and view.mesh.get_custom_aabb().position.y == -130, "Updated AABB and Y flip")
	var saved := bridge.get_mesh_snapshot(Fixture.MESH)
	var revision_saved: int = bridge.get_document_summary().revision
	check(bridge.rebuild_preview().ok, "Rebuild derived caches")
	check(bridge.get_mesh_snapshot(Fixture.MESH) == saved, "Rebuild never modifies source")
	check(bridge.get_document_summary().revision == revision_saved, "Rebuild never changes document revision")
	check(bridge.get_mesh_view(Fixture.MESH).get_positions_snapshot() == saved.base_positions, "Rebuilt positions match source")
	check(bridge.get_child_count() == 1, "Exactly one view after rebuild")
	for i in 12:
		bridge.rebuild_preview()
	check(bridge.get_child_count() == 1, "No accumulating cache views")
	check(not bridge.create_mesh({"id": "wrong"}).ok, "Malformed descriptor rejected")
	check(not bridge.set_vertex_positions(Fixture.MESH, PackedInt64Array([-1]), PackedVector2Array([Vector2.ZERO])).ok, "Invalid integer vertex ID rejected")
	# Inject cache loss; a preview error must not be misreported as a source rollback.
	bridge.get_mesh_view(Fixture.MESH).clear()
	var lost_preview := bridge.set_vertex_positions(Fixture.MESH, PackedInt64Array([20]), PackedVector2Array([Vector2(130, 140)]))
	check(lost_preview.ok and not lost_preview.preview_ok, "Separate source commit from preview failure")
	check(bridge.get_mesh_snapshot(Fixture.MESH).base_positions[3] == Vector2(130, 140), "Source remains committed after cache loss")
	check(bridge.rebuild_preview().ok, "Recover from lost preview resources")
	check(bridge.get_mesh_view(Fixture.MESH).get_positions_snapshot()[3] == Vector2(130, 140), "Recovery uses latest source")
	bridge.free()
	check(tex.get_reference_count() == texture_refs, "Document bridge destruction releases texture references")
	await process_frame
	print("KASANE_INTEGRATION_TEST_", "OK" if failures == 0 else "FAILED", ": ", checks, " checks, ", failures, " failures")
	quit(0 if failures == 0 else 1)
