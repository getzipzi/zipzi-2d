extends Control

const CanvasSurface = preload("res://canvas.gd")
var canvas: Control
var status: Label
var output: RichTextLabel

func label_for(text: String, font_size: int = 14) -> Label:
	var label := Label.new()
	label.text = text
	label.add_theme_font_size_override("font_size", font_size)
	return label

func button_for(parent: Control, text: String, action: Callable, unavailable: String = "") -> Button:
	var button := Button.new()
	button.text = text
	button.disabled = not unavailable.is_empty()
	button.tooltip_text = unavailable
	button.pressed.connect(action)
	parent.add_child(button)
	return button

func panel(parent: Control, title: String, width: float = 240) -> VBoxContainer:
	var box := VBoxContainer.new()
	box.custom_minimum_size.x = width
	box.add_theme_constant_override("separation", 12)
	parent.add_child(box)
	box.add_child(label_for(title, 18))
	box.add_child(HSeparator.new())
	return box

func empty_text(parent: Control, text: String) -> void:
	var label := label_for(text)
	label.autowrap_mode = TextServer.AUTOWRAP_WORD_SMART
	label.modulate = Color("9caac0")
	parent.add_child(label)

func report(text: String) -> void:
	status.text = text
	output.append_text(text + "\n")

func _ready() -> void:
	var skin := Theme.new()
	skin.default_font_size = 14
	var style := StyleBoxFlat.new()
	style.bg_color = Color("253149")
	style.set_corner_radius_all(5)
	style.content_margin_left = 12
	style.content_margin_right = 12
	style.content_margin_top = 8
	style.content_margin_bottom = 8
	skin.set_stylebox("normal", "Button", style)
	theme = skin
	var margin := MarginContainer.new()
	margin.set_anchors_and_offsets_preset(Control.PRESET_FULL_RECT)
	for side in ["left", "right", "top", "bottom"]:
		margin.add_theme_constant_override("margin_" + side, 16)
	add_child(margin)
	var layout := VBoxContainer.new()
	layout.add_theme_constant_override("separation", 12)
	margin.add_child(layout)
	var header := HBoxContainer.new()
	layout.add_child(header)
	header.add_child(label_for("KASANE  /  Viewer", 24))
	var spacer := Control.new()
	spacer.size_flags_horizontal = Control.SIZE_EXPAND_FILL
	header.add_child(spacer)
	header.add_child(label_for("APPLICATION SHELL  /  尚未接入模型", 13))
	var toolbar := HBoxContainer.new()
	layout.add_child(toolbar)
	button_for(toolbar, "打开 model3.json", func(): pass, "等待运行时加载与 M4 renderer 接入")
	button_for(toolbar, "打开 MOC3 + 纹理映射", func(): pass, "等待运行时与显式纹理槽映射接入")
	button_for(toolbar, "保存模型截图", func(): pass, "等待模型绘制就绪与截图接口")
	var picker := ColorPickerButton.new()
	picker.color = Color("171d29")
	picker.custom_minimum_size.x = 48
	picker.tooltip_text = "画布背景"
	picker.color_changed.connect(func(color: Color):
		canvas.background = color
		canvas.queue_redraw())
	toolbar.add_child(picker)
	var body := HSplitContainer.new()
	body.size_flags_vertical = Control.SIZE_EXPAND_FILL
	layout.add_child(body)
	var left := panel(body, "运行模型", 230)
	empty_text(left, "尚未加载模型\n\n路径：—\nMOC3 版本：—\n画布：—\n纹理槽：—")
	left.add_child(HSeparator.new())
	empty_text(left, "model3.json 引用资源\n或 MOC3 + 显式纹理槽映射\n\n运行时支持范围独立于 Editor 导入范围。")
	var right_split := HSplitContainer.new()
	right_split.size_flags_horizontal = Control.SIZE_EXPAND_FILL
	body.add_child(right_split)
	var center := VBoxContainer.new()
	center.size_flags_horizontal = Control.SIZE_EXPAND_FILL
	right_split.add_child(center)
	var navigation := HBoxContainer.new()
	center.add_child(navigation)
	button_for(navigation, "重置视图", func(): canvas.reset_view())
	button_for(navigation, "适配内容", func(): pass, "等待模型和公共 renderer 接入（M4）")
	navigation.add_child(label_for("滚轮缩放 · 中键平移", 12))
	canvas = CanvasSurface.new()
	canvas.custom_minimum_size = Vector2(320, 220)
	canvas.size_flags_vertical = Control.SIZE_EXPAND_FILL
	center.add_child(canvas)
	var overlay := CenterContainer.new()
	overlay.set_anchors_and_offsets_preset(Control.PRESET_FULL_RECT)
	overlay.mouse_filter = Control.MOUSE_FILTER_IGNORE
	canvas.add_child(overlay)
	var hint := label_for("等待打开运行模型\nPurismCore + 公共 renderer 待接入", 20)
	hint.horizontal_alignment = HORIZONTAL_ALIGNMENT_CENTER
	hint.mouse_filter = Control.MOUSE_FILTER_IGNORE
	overlay.add_child(hint)
	var right := panel(right_split, "参数预览", 260)
	empty_text(right, "暂无参数\n\n加载后显示 ID、范围、默认值与当前采样值。")
	button_for(right, "恢复参数默认值", func(): pass, "等待运行时参数接口")
	var tabs := TabContainer.new()
	tabs.custom_minimum_size.y = 155
	layout.add_child(tabs)
	output = RichTextLabel.new()
	output.name = "运行反馈"
	output.scroll_following = true
	tabs.add_child(output)
	
	status = label_for("模型：未打开  |  Runtime：未连接", 12)
	layout.add_child(status)
	report("Viewer 应用壳已启动。不加载 Document 或编辑脚本宿主。M6 尚未验收。")
