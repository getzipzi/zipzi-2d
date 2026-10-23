# 当前架构

```text
Python wheel (kasane.Session) → kasane-sdk → kasane-core / kasane-project / kasane-moc3
             │                     │
             └── kasane.Observer → kasane-sdk-observe → kasane-project
                                                        ↓
                                DrawableFrame → kasane-render → kasane-render-metal (macOS)
                                                         └────────→ kasane-render-wgpu (其他平台)
                                                                      ↓
                                                              PNG / focus crop
```

`kasane-sdk` 提供隔离编辑、版本检查、undo/redo、导入、工程保存和 MOC3 导出；Python wheel 暴露脚本入口。`kasane-sdk-observe` 从已发布或未发布的会话制作只读快照，通过 `kasane-project` 读取并验证图像资源，再由 macOS 上的原生 `kasane-render-metal` 或其他平台的 `kasane-render-wgpu` 输出帧与诊断资料。两个后端复用 `kasane-render` 的场景规划与 CPU 几何处理。

文档 Undo/Redo 统一由 `kasane-sdk::AuthoringSession` 的检查点历史管理。
`kasane-core` 提供完整持久内容的检查点、候选发布和恢复；`kasane-project::DocumentSession`
负责工程与资源读写，不保存历史栈。交互编辑在隔离候选中预览，提交时生成一个历史步骤；
取消、无变化或失败的编辑保留原有 redo 分支，保存由 SDK 协调历史资源路径与已保存基线。

Motion 描述与曲线负载分别复制：`MotionClip` 的 tracks / events 使用
`SharedVec`，描述字段修改保留共享负载，曲线编辑通过写时复制分离。
类型化标量 patch 只检查修改字段及其依赖；Fade 无需重复遍历曲线，
Duration / FPS 需要检查既有轨道末端和事件的时间边界、半帧容差与源时间范围。
SDK 正式发布仍执行完整结构校验。共享存储保持原有 JSON 数组编码，历史内存预算
按每个检查点完整计费，避免共享分配导致低估。

## SDK 与 Python 包的职责

`kasane-python` 是一个发行包，内部同时包含 PyO3 Rust 扩展和纯 Python 代码；它不是与 `kasane-sdk` 并列的第二套工程内核。区分代码位置时看**谁拥有规则、谁会使用规则**，不以实现语言或文件长度决定。

| 位置 | 负责什么 | 本仓库实例 |
| --- | --- | --- |
| `kasane-core` / `kasane-project` 等底层 crate | 文档数据、求值及格式规则 | mesh、binding、画布与工程格式 |
| `kasane-sdk`（Rust） | 跨语言一致的编辑契约：事务、依赖验证、版本、历史、导入导出 | `EditSession::replace_topology` 在一次编辑中检查并替换 mesh 与关联对象 |
| `kasane-python/src`（PyO3） | 将 Python 值转成 Rust 类型，调用 Rust SDK，转换错误与结果；不维护另一套文档状态 | `NativeSession`、`NativeEdit` |
| `kasane-python/python/kasane` | 面向 Python 用户的类型、快照、上下文管理器、路径习惯和工作流 | `Session`、`Edit`、批量观察与报告；建模方法包装 Rust API |
| `docs/experiments/` | 依赖特定模型、映射或尚未验证为通用能力的一次性实验 | Shirousagi 的参考位移投影、头部图层选择和视觉比较 |

新增能力先确定规则所有权，再选择语言。决定模型变化、求值或形变保持的正式算法由 Rust 体系实现，即使当前只有 Python 调用者；文档不变量归 core，事务与建模操作归 SDK，持久化归 project。Python 可组合已有操作完成批处理、报告与脚本便利语法，但组合中若产生新的建模规则，也应交由 Rust 维护。尚未稳定的模型专属算法保留在实验中。

正式验收使用仓库外安装的 Python wheel、Rust 契约与 GPU 测试、官方/Purism Core 数值探针，以及仓库内固定的外部 GPU 参考图。参考图的输入哈希和来源记录在 `tests/fixtures/render_reference/`；[验证命令](VALIDATION.md)会检查输入与参考图身份。

Godot 编辑器、Viewer、demo 与 GDExtension 已从代码库移除。旧实现方案和里程碑记录见 [archive](archive/)。

## CMO3 / CAN3 导出与转换边界

`kasane-cmo3` 负责 `Document + AssetResolver → Cmo3Artifact`，独立处理 CAFF、
Cubism XML 对象图和模型语义映射，不依赖 project、编辑器、JVM 或官方 SDK。
`kasane-project` 负责已保存工程和内存会话的资源校验，以及暂存/原子发布；
`kasane-sdk` 暴露会话编码入口。诊断与文件字节一起返回，默认拒绝已知有损转换。
`encode_can3` 把 motion clips 写为可编辑动画场景，并按相对文件名链接 CMO3；
`encode_cubism` 成对返回模型、动画和合并诊断，在任何资源读取前完成有损预检。
整数帧/毫秒量化、冲突、未支持的动画目标都会显式报告。项目层提供成对目录发布，
暂存完整文件后提交，提交失败恢复旧目录；拒绝覆盖含非导出文件的目录。
未来 CLI 以 `Document` 和资源接口组合 Kasane/MOC3/PSD 导入与 CMO3/CAN3 导出，
格式能力与限制见 [crate 文档](../crates/kasane-cmo3/README.md)。

模型与视口坐标、格式边界及导出设置见 [坐标契约](coordinates.md)。
