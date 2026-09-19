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
	header.add_child(label_for("KASANE  /  Editor", 24))
	var spacer := Control.new()
	spacer.size_flags_horizontal = Control.SIZE_EXPAND_FILL
	header.add_child(spacer)
	header.add_child(label_for("APPLICATION SHELL  /  尚未接入模型", 13))
	var toolbar := HBoxContainer.new()
	layout.add_child(toolbar)
	for item in ["新建", "打开", "保存", "另存为", "导入 PNG", "导入 MOC3", "导出 MOC3"]:
		button_for(toolbar, item, func(): pass, "等待 Document / 工程文件 / 导入导出接口（M1–M3、M5）")
	var body := HSplitContainer.new()
	body.size_flags_vertical = Control.SIZE_EXPAND_FILL
	layout.add_child(body)
	var left := panel(body, "对象与组织", 230)
	var tree := Tree.new()
	tree.size_flags_vertical = Control.SIZE_EXPAND_FILL
	tree.custom_minimum_size.y = 160
	left.add_child(tree)
	tree.create_item().set_text(0, "尚无 Document")
	empty_text(left, "Part / ArtMesh / Deformer\n\n组织树与变形父关系将分别显示。")
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
	var hint := label_for("等待创建或打开工程\nDocument 与 renderer 待接入", 20)
	hint.horizontal_alignment = HORIZONTAL_ALIGNMENT_CENTER
	hint.mouse_filter = Control.MOUSE_FILTER_IGNORE
	overlay.add_child(hint)
	var right := panel(right_split, "属性检查", 260)
	empty_text(right, "未选择对象\n\nID · 类型 · 素材 · 基础属性\n绑定 · Keyform · 资源诊断")
	right.add_child(HSeparator.new())
	right.add_child(label_for("参数预览", 18))
	empty_text(right, "暂无参数。预览值属于临时状态，不写入 Keyform。")
	button_for(right, "恢复参数默认值", func(): pass, "等待参数数据接口（M1）")
	var tabs := TabContainer.new()
	tabs.custom_minimum_size.y = 155
	layout.add_child(tabs)
	output = RichTextLabel.new()
	output.name = "运行反馈"
	output.scroll_following = true
	tabs.add_child(output)
	var script_panel := VBoxContainer.new()
	script_panel.name = "Agent 脚本"
	tabs.add_child(script_panel)
	empty_text(script_panel, "本地请求 / 结果目录入口待 M5 接入；当前不执行脚本。\n执行 ID、文档代次、起止 revision、错误位置与观察产物将在此显示。")
	button_for(script_panel, "执行脚本", func(): pass, "等待 Document 绑定和脚本错误捕获（M5）")
	status = label_for("工程：未打开  |  Document：未连接", 12)
	layout.add_child(status)
	report("Editor 应用壳已启动。M1–M5 功能尚未验收。")
