# Kasane Editor

基于 **GPUI Kit + kasane-sdk + kasane-render-metal** 的 macOS 编辑器。

已确认的目标设计见 [编辑器基础架构](docs/archive/editor-foundation.md) 和 [Document 面板与参数绑定设计](docs/document-panels-design.md)，性能输入见 [基准模型](models/README.md)。面板与参数绑定设计尚未实现，下文描述当前实现。

```text
crates/editor                EditorView、Kit 界面、文件对话框与 GPUI 预览合成
crates/editor-core           Editor 状态协调、SDK 文档会话、编辑命令、CPU 场景快照与视口状态
crates/editor-render         原生 Metal 渲染与纹理缓存，不包含文档编辑和项目 IO
crates/component-playground  Kit 组件示例、状态反馈与 Rust 用法
```

## 运行

需要 macOS、Rust stable、Xcode Command Line Tools，以及相邻的 Kasane checkout：

```text
web/
├── kasane-editor/
└── kasane-2d/       https://github.com/pakholeung37/kasane-2d
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
./scripts/build-app.sh editor debug  # 显式打包未优化的调试版
```

应用输出为 `target/Kasane Editor.app` 和 `target/Kasane Component Playground.app`，默认使用 Release 优化。无需后端 feature 参数。

监听编译需要先安装 `watchexec`（macOS 可运行 `brew install watchexec`）。脚本监听本仓库及相邻 Kasane 模块中的 Rust 源码和 Cargo 清单，保存后编译；只有编译成功才在后台启动新版并关闭旧版，避免新窗口抢走当前应用的焦点。编译失败时旧版继续运行。关闭当前编辑器或 Playground 窗口后，脚本会停止监听并退出；源码更新导致的正常重启会继续监听。完整编译输出写入 `target/dev-build-*.log`，应用输出写入 `target/dev-run-*.log`。这是进程重启，编辑器中尚未保存的文档更改会丢失；改 UI 前先保存文档。脚本仅在本次开发会话开启增量编译，不改动项目的 Cargo profile。

## 调试界面与性能

120 fps 的帧预算为 8.33 ms。性能验证使用 `cargo run --locked --release -p kasane-editor` 或默认打包的 Release 应用；开发构建保留快速编译和调试能力。基准方法和测量结果见 [交互性能](docs/interaction-performance.md)。实际窗口的输入、异步预览、GPUI 合成和显示时间关联测试见 [窗口 benchmark](docs/window-benchmark.md)。

Parameters slider 的真实拖动测试和优化结果见 [参数面板性能](docs/parameter-slider-performance.md)；可用 `KASANE_BENCH_PARAMETERS=1 scripts/window-benchmark.sh MODEL OUTPUT_DIR 360 120` 复现。

编辑器的上述 debug 启动命令和 `./scripts/dev.sh` 会启用 macOS Metal HUD，放在左下角，宽度为默认值的 80%。按 **⌘⌥I** 打开 GPUI Kit Inspector，点选元素查看布局、样式和源码位置。GPUI FPS 面板暂时停用；需要时可给编辑器单独加 `--features debug-tools` 重新开启。

Apple 的 `MTL_HUD_SCALE` 表示 HUD 宽度占窗口宽度的比例，默认值为 `0.2`，因此 `0.16` 对应默认宽度的 80%。Metal HUD 分别显示应用内存与 Metal 资源分配量。空闲窗口不会仅为更新 HUD 而定时重绘；拖动画布时 HUD 会随正常帧更新。

## 渲染与交互

GPUI 使用原生 Metal；Kasane 共享 GPUI 的 device/queue，后台绘制不可变 RGBA 纹理，由 GPUI 按正常场景顺序直接合成。显示路径没有 CPU 图像回读。预览分辨率随画布区域和显示缩放倍率调整；导入纹理使用线性过滤和 mipmap。

选中网格的边线和顶点标记默认由专用 Metal 覆盖层绘制为透明纹理，再交给 GPUI 合成；保留选中、悬停、裁剪和像素对齐。内容未变时复用纹理，变化时发布独立资源，不在 UI 线程等待 GPU。设备初始化失败时回退到 GPUI。实现和回归检查见 [覆盖层重构](docs/mesh-overlay-rendering.md)，方案对照见 [覆盖层 benchmark](docs/mesh-overlay-benchmark.md)。

编辑器启动时显示空白项目，不预置图层或纹理。打开或导入项目后，图层列表来自 SDK 文档，支持选择网格并编辑名称、透明度和可见性。

- 名称输入实时预览，Enter 或移开焦点提交，Esc 取消；一次输入会话形成一条历史记录。
- 透明度拖动实时预览，释放鼠标后提交为一次编辑。撤销与重做同时恢复属性和画布。
- 打开 Kasane 项目目录或 `project.kasane.json`；保存/另存为通过 SDK 发布项目目录和纹理资源，保留当前撤销历史。
- 从原生“文件”菜单导入 `model3.json` 或分层 PSD。model3 成为待保存的文档；PSD 导入时选择新项目目录，由 SDK 写入图层和纹理。
- 缩放和重置视图只影响视口，不修改模型，也不进入撤销历史。
- macOS 原生菜单栏提供文件、编辑、视图和窗口菜单。支持 ⌘O 打开、⌘S 保存、⇧⌘S 另存为、⌘Z 撤销、⇧⌘Z 重做，以及 ⌘+/⌘−/⌘0 缩放和重置视图。
- 名称输入框获得焦点时，编辑菜单操作文本（包括剪切、拷贝、粘贴、全选）；点击画布后，撤销/重做操作文档。⌘W 关闭活动模型，Ctrl-Tab / Ctrl-Shift-Tab 切换模型；⌘Q 按文档处理未保存状态后退出。
- 新建、打开、导入成功后新增模型标签页；重复打开同一项目激活已有标签页，失败保留当前文档。关闭脏文档提供保存、放弃、取消，关闭最后标签后保留空工作区；当前版本要求项目纹理完整可读。

- 中央模型标签页固定，图层与属性面板使用 Kit Dock，可拖拽、分组、拆分、折叠；布局自动保存，窗口菜单可显示/隐藏面板和重置布局。
- 画布中按 V 激活基础顶点工具；点击选择、Shift 点击切换多选、拖动后一次提交，Esc 或失焦取消。仅允许无参数、变形器、BlendShape、Glue 依赖的基础网格编辑。
- 中键拖动平移，缩放/重置与模型、控制点、命中共用实测画布坐标。顶点拖动期间禁用缩放；中键导航先取消拖动。
- 快捷键可通过应用配置覆盖或取消绑定；编辑菜单提供快捷键清单及恢复默认。格式与失败回退见 [实施记录](docs/editor-foundation-implementation.md)。

Input、Checkbox、Slider、Button、Dock、主题和 Root 使用 Kit 原有实现。项目打开、导入和保存均在后台执行；保存期间只冻结对应文档，其他标签页仍可操作。详细分层见 [文档编辑架构](docs/document-editing.md)，实现记录与未验证交互见 [实施记录](docs/editor-foundation-implementation.md)。

Playground 提供 Button、Checkbox、Input、Slider、Tabs、Dialog、Typography 七页。UI 交互与视觉验证由用户手动完成，不使用 computer-use。

只保留三个必要的 GPUI vendor 包，补丁用于原生 Metal 资源访问和纹理合成；Kit 组件未修改。见 [依赖说明](DEPENDENCIES.md)、[GPUI 补丁范围](vendor/README.md)、[Metal 集成与测试](docs/native-metal.md)。
