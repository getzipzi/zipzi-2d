# Native project macOS interaction acceptance

日期：2026-11-04。macOS 27.0.1 arm64，Rust 1.99.0，实际 Editor debug 应用进程。
基线 `0a1c30f5` 加保存对话框修复；可执行文件 SHA-256 与逐项结果见
[JSON 记录](2026-11-04-native-project-format-macos.json)。

通过独立 bundle 标识、私有 Runtime/config/cache/session 目录和工程副本进行验证。
`open -a` 经过真实 LaunchServices；Connected 客户端核对 host PID、Session 身份和内容，
辅助功能接口核对原生 Save/Open 面板。测试仅结束自己启动的进程，不改变默认打开应用。

| 场景 | 证据 |
| --- | --- |
| 冷启动、多文件、启动中追加事件 | 两个文件随冷启动送入，ready 文件出现前再送入第三个；三个归档均进入 ready Session |
| 中文、空格、前导 `-` 和 `.ZIPZI` 路径 | 三个路径完整保留，两个 Mesh 与全部有效资源正常 |
| 相同文档 UUID 的不同副本 | 三个不同路径保留三个独立 Session |
| 运行中重复文件事件 | 激活原文档，Session 身份不变，无重复 Session |
| 无窗口 host 接收文件事件 | WindowServer 初始窗口数为零，随后创建窗口并打开三份工程 |
| 第二个实际 Editor 进程 | 返回 `reused:<原 host PID>`，经 IPC 激活目标文档，Session 身份不变 |
| 最近工程 | 只记录用户归档路径，输入文件字节未改变 |
| 原生 Save As 默认值与取消 | 建议名为工程名加 `.zipzi`，目录为原归档父目录；取消不保存 |
| 输入其他后缀 | 输入 `wrong.png` 后先出现系统 `.png` / `.zipzi` 纠正提示；取消不生成文件 |
| 无后缀名称碰撞及取消覆盖 | `overwrite` 在规范化为 `overwrite.zipzi` 后触发原生 Replace 确认；取消保持目标字节与 Session 路径 |
| 无后缀、大小写后缀、显式双后缀 | 三种另存为均核对完整 authoring JSON 与全部 PNG 字节；返回路径与确认路径一致 |
| 原生 Open 取消 | 面板正常打开、取消后关闭 |

原生测试发现原封装会静默把 `wrong.png` 改成 `wrong.png.zipzi`，且无后缀碰撞没有在
原生面板确认最终目标。修复改用 `NSOpenSavePanelDelegate` 在 AppKit 替换检查之前补齐
缺失后缀，允许系统提示其他后缀；带过滤的保存不再经过旧的 Sequoia 后缀回写修正。
回调完整持有 weak delegate 的强引用，直到原生面板完成。

复现（输出目录必须不存在；`--dialogs` 需要已有辅助功能权限和英文系统面板）：

```sh
env -u ZIPZI_DEV_SESSION_DIR cargo build --locked -p zipzi-editor
./tools/dev-python tools/runtime/native_project_macos.py target/debug/zipzi-editor \
  --dialogs --output target/native-project-macos-new
```

另通过 release 元数据工具 6 项、开发 bundle 工具 6 项，以及实际 macOS 原生构建。

本记录验证指定应用的 LaunchServices 文档事件，不等同于 Finder 默认关联双击。
正式 bundle 的默认类型选择、Finder 双击，以及 Windows 原生构建、关联注册/卸载、
占用句柄、no-replace 和原生对话框/Explorer 状态矩阵仍待对应环境验收。
因此整个 plan 的两平台最终退出条件尚未满足。
