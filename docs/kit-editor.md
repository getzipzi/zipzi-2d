# GPUI Kit editor

## 选择与边界

2026-09-27：尝试将 Kit 0.6.5 的依赖替换为当前 gpui-ce 时，`gpui-base` 出现 134 个编译错误，主要涉及颜色类型、TextRun、PaintQuad、浮层与事件 API。用户选择先使用官方 GPUI 组装独立原型，允许渲染后端变化。因此没有移植或修改 Kit/Base，也没有修改 gpui-ce fork。

入口：`crates/editor/src/main.rs`。Kit 固定到官方 commit `5059de47a6560fdbe24e32cd91b45d6aa6849695`，GPUI 为 Kit 固定的 `gpui-pre 0.3.6`。随后按用户要求全面迁移：主 editor 和 component-playground 统一使用 Kit，删除旧手写 ui crate，合并到根 workspace 与 lockfile。

```sh
cargo run --locked -p kasane-editor
./scripts/build-app.sh
cargo test --locked --workspace
cargo fmt --all -- --check
```

## 组件与状态

- 使用 `gpui_kit::init`、官方 Assets 与 `open_window`（包含 Root），由 Kit 管理输入、键盘导航和浮层。
- Input / InputState 保存图层名称，Change 事件刷新图层列表；不额外实现按键或 IME 文本插入。
- Checkbox 控制 Kasane Document 的 visible。
- Slider / SliderState 通过 Change 事件控制 opacity。
- Button 控制缩放、重置预览；重置恢复缩放、透明度、可见性，保留名称。
- 主题使用 Kit semantic tokens；关于 Dialog 使用官方窗口浮层。
- 窗口最小尺寸 800×480，左右面板固定 rem 宽度，中间画布自动伸缩。未实现停靠、面板拖动、文件保存、模型导入或撤销。

## 渲染桥接

UI：官方 GPUI macOS Metal 后端。

场景：后台线程创建独立 wgpu 29 device，复用 `editor-scene` 的 Kasane 渲染器，渲染固定 768×768 透明预览。GPU 回读后将 premultiplied RGBA 转为 straight BGRA，再创建 GPUI RenderImage。只在场景状态变化时重新渲染；快速滑块请求合并，UI 不同步等待 GPU。

这条路径有 CPU 回读和重新上传，不能据此推断共享设备性能。缩放窗口时 GPUI 缩放预览，超出预览分辨率可能变软。旧共享纹理实现仅保留在 Git 历史中，当前构建不包含 gpui-ce。

## 本机验证

Apple M4 / macOS，2026-09-27：

- 全工作区测试通过：场景 GPU 测试与图像桥接 GPU 测试各一项；格式检查通过。
- 主 editor 的正式应用入口构建、打包和窗口预览验证通过。
- GPU 桥接测试通过：核对 BGRA 通道顺序、半透明颜色/alpha、隐藏后的全透明像素。实测发现并修复了重复乘 alpha 导致半透明颜色发灰的问题。
- 实际窗口：首帧、名称输入及左栏联动、中文文本粘贴、输入框垂直居中、显示/隐藏、缩放、透明度轨道点击、浅/深主题、关于弹窗打开与 Escape 关闭均验证。
- 从 Input 按 Tab，再按 Space，可切换 Checkbox。
- 约 800×480 和 1100×740 内容尺寸检查，控件和画布保持可见。
- 中文粘贴不是 IME 测试。尚未验证系统中文输入法的候选窗、组合文本和连续上屏，请在名称框中实测；不能宣称之前的重复上屏问题已被这次原型验证解决。
- Slider 的 CUA accessibility setValue 未改变状态；轨道指针点击正常。未验证其方向键操作、所有控件的屏幕阅读器体验或完整焦点恢复。

编译存在上游 `block 0.1.6` future-incompatibility 提示，目前不阻止构建。Linux / Windows 未验证。
