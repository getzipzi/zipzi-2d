extends Control
## Shell-only navigation surface. M4 will supply model drawing.
var zoom: float = 1.0
var offset := Vector2.ZERO
var background := Color("171d29")

func _ready() -> void:
	mouse_default_cursor_shape = Control.CURSOR_MOVE
	clip_contents = true
	resized.connect(queue_redraw)

func reset_view() -> void:
	zoom = 1.0
	offset = Vector2.ZERO
	queue_redraw()

func _gui_input(event: InputEvent) -> void:
	if event is InputEventMouseButton and event.pressed:
		var factor := 1.0
		if event.button_index == MOUSE_BUTTON_WHEEL_UP:
			factor = 1.1
		elif event.button_index == MOUSE_BUTTON_WHEEL_DOWN:
			factor = 1.0 / 1.1
		if factor != 1.0:
			var next_zoom := clampf(zoom * factor, 0.1, 8.0)
			var anchor: Vector2 = event.position - size / 2.0
			offset = anchor - (anchor - offset) * next_zoom / zoom
			zoom = next_zoom
			queue_redraw()
			accept_event()
	if event is InputEventMouseMotion and event.button_mask & MOUSE_BUTTON_MASK_MIDDLE:
		offset += event.relative
		queue_redraw()
		accept_event()

func _draw() -> void:
	draw_rect(Rect2(Vector2.ZERO, size), background)
	var center := size / 2.0 + offset
	var step := 48.0 * zoom
	for i in range(int(size.x / step) + 2):
		var x := fposmod(center.x, step) + i * step
		draw_line(Vector2(x, 0), Vector2(x, size.y), Color(1, 1, 1, 0.035))
	for i in range(int(size.y / step) + 2):
		var y := fposmod(center.y, step) + i * step
		draw_line(Vector2(0, y), Vector2(size.x, y), Color(1, 1, 1, 0.035))
	draw_line(Vector2(center.x, 0), Vector2(center.x, size.y), Color(0.4, 0.6, 1, 0.2))
	draw_line(Vector2(0, center.y), Vector2(size.x, center.y), Color(0.4, 0.6, 1, 0.2))
