# GPUI Kit editor

## 选择与边界

2026-09-27：尝试将 Kit 0.6.5 的依赖替换为当前 gpui-ce 时，`gpui-base` 出现 134 个编译错误，主要涉及颜色类型、TextRun、PaintQuad、浮层与事件 API。用户选择先使用官方 GPUI 组装独立原型，允许渲染后端变化。因此没有移植或修改 Kit/Base，也没有修改 gpui-ce fork。

入口：`crates/editor/src/main.rs`。Kit 固定到官方 commit `5059de47a6560fdbe24e32cd91b45d6aa6849695`，GPUI 为 Kit 固定的 `gpui-pre 0.3.6`。随后按用户要求全面迁移：主 editor 和 component-playground 统一使用 Kit，删除旧手写 ui crate，合并到根 workspace 与 lockfile。

```sh
cargo run --locked -p zipzi-editor
./scripts/build-app.sh
cargo test --locked --workspace
cargo fmt --all -- --check
```

## 组件与状态

- 使用 `gpui_kit::init`、官方 Assets 与 `open_window`（包含 Root），由 Kit 管理输入、键盘导航和浮层。
- Input / InputState 保存图层名称，Change 事件刷新图层列表；不额外实现按键或 IME 文本插入。
- Checkbox 控制 Zipzi Document 的 visible。
- Slider / SliderState 通过 Change 事件控制 opacity。
- Button 控制缩放、重置预览；重置恢复缩放、透明度、可见性，保留名称。
- 主题使用 Kit semantic tokens；关于 Dialog 使用官方窗口浮层。
- 窗口最小尺寸 800×480，左右面板固定 rem 宽度，中间画布自动伸缩。未实现停靠、面板拖动、文件保存、模型导入或撤销。

## 渲染桥接

使用 GPUI 原生 Metal 与 zipzi-render-metal，共享 device/queue。后台生成不可变 RGBA 纹理，通过 GPUI Surface 按场景顺序合成，没有 CPU 像素回读。无需 feature 参数。

用户已确认原生 Metal 版本运行正常。自动验证保留真实 GPU 离屏合成测试，UI/输入法与窗口行为交由用户手动验证。详细实现和限制见 [native-metal.md](native-metal.md)。
