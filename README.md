# Kasane Editor

最小 Rust monorepo，验证 **GPUI Kit + kasane-render-metal** 的 macOS editor 组装。

```text
crates/editor                Kit 工具栏、名称输入、属性栏与 Kasane 预览
crates/editor-scene          独立于 UI 的 Kasane 场景与原生 Metal 渲染
crates/component-playground  Kit 组件示例、状态反馈与 Rust 用法
```

## 运行

需要 macOS、Rust stable、Xcode Command Line Tools，以及相邻的 Kasane checkout：

```text
web/
├── kasane-editor/
└── kesane-2d/       https://github.com/pakholeung37/kasane-2d
```

Kasane checkout 必须包含 `modules/kasane-render-metal` 及对应的公共渲染模块修改。GPUI Kit 使用官方固定 commit，不需要本地 gpui-ce 或 gpui-kit checkout。

```sh
cargo run --locked -p kasane-editor
cargo run --locked -p component-playground
cargo test --locked --workspace
cargo fmt --all -- --check

./scripts/build-app.sh
./scripts/build-app.sh playground
```

应用输出为 `target/Kasane Editor.app` 和 `target/Kasane Component Playground.app`。无需后端 feature 参数。

## 渲染与交互

GPUI 使用原生 Metal；Kasane 共享 GPUI 的 device/queue，后台绘制不可变 RGBA 纹理，由 GPUI 按正常场景顺序直接合成。显示路径没有 CPU 图像回读。预览固定为 768×768，放大窗口时可能变软。

Input、Checkbox、Slider、Button、Dialog、主题和 Root 都使用 Kit 原有实现。名称输入同步图层列表；缩放、透明度和可见性更新场景。所有更改仅保留在当前窗口，尚无保存、导入、撤销、时间轴或停靠布局。

Playground 提供 Button、Checkbox、Input、Slider、Tabs、Dialog、Typography 七页。UI 交互与视觉验证由用户手动完成，不使用 computer-use。

只保留三个必要的 GPUI vendor 包，补丁用于原生 Metal 资源访问和纹理合成；Kit 组件未修改。见 [依赖说明](DEPENDENCIES.md)、[GPUI 补丁范围](vendor/README.md)、[Metal 集成与测试](docs/native-metal.md)。
