extends RefCounted

const DOC := "11111111-1111-4111-8111-111111111111"
const ASSET := "22222222-2222-4222-8222-222222222222"
const MESH := "33333333-3333-4333-8333-333333333333"
const RED := Color(0.92, 0.25, 0.35, 1)
const BLUE := Color(0.25, 0.65, 0.96, 1)
const GREEN := Color(0.25, 0.85, 0.64, 1)
const YELLOW := Color(1.0, 0.77, 0.30, 1)

static func texture() -> ImageTexture:
	# Generated locally: no SDK, model, image import, or external artwork needed.
	var image := Image.create(32, 32, false, Image.FORMAT_RGBA8)
	image.fill_rect(Rect2i(0, 0, 16, 16), RED)
	image.fill_rect(Rect2i(16, 0, 16, 16), BLUE)
	image.fill_rect(Rect2i(0, 16, 16, 16), GREEN)
	image.fill_rect(Rect2i(16, 16, 16, 16), YELLOW)
	return ImageTexture.create_from_image(image)

static func mesh() -> Dictionary:
	return {
		"id": MESH, "name": "Memory quad", "texture_asset_id": ASSET,
		"vertex_ids": PackedInt64Array([40, 10, 90, 20]),
		"base_positions": PackedVector2Array([Vector2(-80, 80), Vector2(-80, -80), Vector2(80, -80), Vector2(80, 80)]),
		"uvs": PackedVector2Array([Vector2(0, 0), Vector2(0, 1), Vector2(1, 1), Vector2(1, 0)]),
		"triangles": PackedInt64Array([40, 10, 90, 40, 90, 20]),
	}

static func populate(bridge: KasaneDocumentBridge, tex: Texture2D) -> Dictionary:
	var result := bridge.initialize(DOC, Vector2(320, 320))
	if not result.ok:
		return result
	result = bridge.add_image_asset(ASSET, "Four quadrants", "res://assets/quadrants.svg", tex)
	if not result.ok:
		return result
	return bridge.create_mesh(mesh())
