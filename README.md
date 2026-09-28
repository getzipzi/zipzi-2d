# Kasane Editor

基于 **GPUI Kit + kasane-sdk + kasane-render-metal** 的 macOS 编辑器。

```text
crates/editor                Kit 界面、文件对话框与 GPUI 预览合成
crates/editor-core           SDK 文档会话、编辑事务、CPU 场景快照与视口状态
crates/editor-render         原生 Metal 渲染与纹理缓存，不包含文档编辑和项目 IO
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
MTL_HUD_ENABLED=1 MTL_HUD_ALIGNMENT=bottomleft MTL_HUD_SCALE=0.16 cargo run --locked -p kasane-editor
cargo run --locked -p component-playground
./scripts/dev.sh                 # 保存后编译成功即在后台重启编辑器
./scripts/dev.sh playground      # 同样方式运行组件 Playground
cargo test --locked --workspace
cargo fmt --all -- --check

./scripts/build-app.sh
./scripts/build-app.sh playground
```

应用输出为 `target/Kasane Editor.app` 和 `target/Kasane Component Playground.app`。无需后端 feature 参数。

监听编译需要先安装 `watchexec`（macOS 可运行 `brew install watchexec`）。脚本监听本仓库及相邻 Kasane 模块中的 Rust 源码和 Cargo 清单，保存后编译；只有编译成功才在后台启动新版并关闭旧版，避免新窗口抢走当前应用的焦点。编译失败时旧版继续运行。关闭当前编辑器或 Playground 窗口后，脚本会停止监听并退出；源码更新导致的正常重启会继续监听。完整编译输出写入 `target/dev-build-*.log`，应用输出写入 `target/dev-run-*.log`。这是进程重启，编辑器中尚未保存的文档更改会丢失；改 UI 前先保存文档。脚本仅在本次开发会话开启增量编译，不改动项目的 Cargo profile。

## 调试界面与性能

编辑器的上述 debug 启动命令和 `./scripts/dev.sh` 会启用 macOS Metal HUD，放在左下角，宽度为默认值的 80%。按 **⌘⌥I** 打开 GPUI Kit Inspector，点选元素查看布局、样式和源码位置。GPUI FPS 面板暂时停用；需要时可给编辑器单独加 `--features debug-tools` 重新开启。

Apple 的 `MTL_HUD_SCALE` 表示 HUD 宽度占窗口宽度的比例，默认值为 `0.2`，因此 `0.16` 对应默认宽度的 80%。Metal HUD 分别显示应用内存与 Metal 资源分配量。debug 版在启用 Metal HUD 时每秒请求一帧，让空闲窗口的读数继续更新；因此空闲 FPS 不为零，也不代表持续动画的性能。

## 渲染与交互

GPUI 使用原生 Metal；Kasane 共享 GPUI 的 device/queue，后台绘制不可变 RGBA 纹理，由 GPUI 按正常场景顺序直接合成。显示路径没有 CPU 图像回读。预览分辨率随画布区域和显示缩放倍率调整；导入纹理使用线性过滤和 mipmap。

编辑器启动时显示空白项目，不预置图层或纹理。打开或导入项目后，图层列表来自 SDK 文档，支持选择网格并编辑名称、透明度和可见性。

- 名称输入实时预览，Enter 或移开焦点提交，Esc 取消；一次输入会话形成一条历史记录。
- 透明度拖动实时预览，释放鼠标后提交为一次编辑。撤销与重做同时恢复属性和画布。
- 打开 Kasane 项目目录或 `project.kasane.json`；保存/另存为通过 SDK 发布项目目录和纹理资源，保留当前撤销历史。
- 从原生“文件”菜单导入 `model3.json` 或分层 PSD。model3 成为待保存的文档；PSD 导入时选择新项目目录，由 SDK 写入图层和纹理。
- 缩放和重置视图只影响视口，不修改模型，也不进入撤销历史。
- macOS 原生菜单栏提供文件、编辑、视图和窗口菜单。支持 ⌘O 打开、⌘S 保存、⇧⌘S 另存为、⌘Z 撤销、⇧⌘Z 重做，以及 ⌘+/⌘−/⌘0 缩放和重置视图。
- 名称输入框获得焦点时，编辑菜单操作文本（包括剪切、拷贝、粘贴、全选）；点击画布后，撤销/重做操作文档。⌘W 关闭、⌘Q 退出沿用未保存提示。
- 打开、导入其他项目或关闭窗口时提示未保存的更改。打开或导入失败保留当前文档；当前版本要求项目纹理完整可读。

Input、Checkbox、Slider、Button、主题和 Root 使用 Kit 原有实现。尚无网格绘制工具、时间轴或停靠布局。项目打开和导入在后台执行；保存目前同步执行，大项目可能短暂阻塞界面。详细分层见 [文档编辑架构](docs/document-editing.md)。

Playground 提供 Button、Checkbox、Input、Slider、Tabs、Dialog、Typography 七页。UI 交互与视觉验证由用户手动完成，不使用 computer-use。

只保留三个必要的 GPUI vendor 包，补丁用于原生 Metal 资源访问和纹理合成；Kit 组件未修改。见 [依赖说明](DEPENDENCIES.md)、[GPUI 补丁范围](vendor/README.md)、[Metal 集成与测试](docs/native-metal.md)。
