# Kasane Editor

基于 **GPUI Kit + kasane-sdk + kasane-render-metal** 的 macOS 编辑器。

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

Kasane checkout 必须包含 `modules/kasane-render-metal`、对应的公共渲染模块修改，以及 SDK 的常规绑定两轴写入限制。GPUI Kit 使用官方固定 commit，不需要本地 gpui-ce 或 gpui-kit checkout。

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

选中网格的边线和顶点标记默认由专用 Metal 覆盖层绘制为透明纹理，再交给 GPUI 合成；保留选中、悬停、裁剪和像素对齐。内容未变时复用纹理，变化时发布独立资源，不在 UI 线程等待 GPU。边线与顶点分层缓存，控制线始终位于标记下方。设备初始化失败时回退到 GPUI。实现和回归检查见 [覆盖层重构](docs/mesh-overlay-rendering.md)，方案对照见 [覆盖层 benchmark](docs/mesh-overlay-benchmark.md)。

只保留三个必要的 GPUI vendor 包，补丁用于原生 Metal 资源访问和纹理合成；Kit 组件未修改。见 [依赖说明](DEPENDENCIES.md)、[GPUI 补丁范围](vendor/README.md)、[Metal 集成与测试](docs/native-metal.md)。
