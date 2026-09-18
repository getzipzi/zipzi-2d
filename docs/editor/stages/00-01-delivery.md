# Stage 00–01 交付与验收记录

日期：2026-09-18。状态：Stage 00、Stage 01 在本记录的范围和环境下已验收。

## 实际交付

- `modules/kasane-core`：独立 C++20 几何校验、Document、稳定身份索引、只读查询、基础位置批量修改和 ChangeSet。
- `modules/gd-kasane`：独立 GDExtension，提供内存网格创建、位置更新与清理，以及 Document 到预览的同步。
- `demos/kasane-preview`：固定视角自动更新样例，内存生成四象限纹理，无外部模型或图片依赖。
- `tools/run_kasane_preview.py`：可重复构建和验收入口。
- [实际架构与 API](../ARCHITECTURE.md)、[Document specification v1](../DOCUMENT_SPEC.md)。

阶段 01 复用了阶段 00 的正式接口，没有新建第二条渲染通路。未修改已有 `gd-cubism` 或 `purism-core` 的源代码。godot-cpp 仍使用原有锁定版本。

## 已验证环境

- macOS arm64，Apple M4。
- Godot `4.7.2.stable.mono.official.ed1daf0bf`。
- AppleClang 21，C++20。
- 真实 GPU 测试使用 Godot Compatibility renderer，OpenGL 4.1 Metal。
- 固定几何：每个视图 4 个顶点、2 个三角形、32×32 内存纹理。

这是功能与资源复用验收，不是大型角色的性能承诺；其他平台尚未验收。

## 验收命令与结果

从仓库根目录运行：

```sh
python3 tools/run_kasane_preview.py --render
```

| 检查 | 实际结果 |
|---|---|
| 核心独立构建与 CTest | 1/1 suite 通过 |
| Godot 集成 | `KASANE_INTEGRATION_TEST_OK: 118 checks, 0 failures` |
| 实际 GPU 像素比较 | `KASANE_RENDER_TEST_OK: 0 failures` |
| 双路径示例 | 已运行、截图并人工检查 |
| 新扩展动态依赖 | `otool -L` 无 Cubism Core/Framework 动态库 |

覆盖内容：

- 内存创建、固定拓扑批量更新、非法初始化和非法更新保持旧结果。
- 位置更新复用网格 RID 和纹理；40 次重建/清理后纹理引用数恢复。
- UUID 唯一性、非顺序顶点 ID 到稠密索引转换、源数组和快照隔离。
- 批次中存在无效顶点或数值时不产生部分修改，revision 不变化。
- 改名不上传顶点、不重建渲染资源。
- 更新包围盒，模型 Y-up 到画布 Y-down 转换、四象限 UV 朝向。
- 直接路径与 Document 路径在更新前的四象限取样正确，更新后的采样像素一致。
- 预览缓存多次重建不增加子节点、不修改源数据和 revision。
- 注入预览资源丢失后，源数据提交与预览错误分别报告，可从最新源数据重建。

日志保存在仓库 `target/kasane/`：`verification.log`、`core-tests.log`、`integration.log`、`render.log`。这些是本地生成产物，不纳入版本控制。

截图同样可由验收命令重新生成：

- `demos/kasane-preview/artifacts/before.png`
- `demos/kasane-preview/artifacts/after.png`
- `demos/kasane-preview/artifacts/demo.png`

## 既有 Cubism 路径回归

使用本机已存在的 Purism 版插件及本地 Mao 素材执行，没有把这些素材加入仓库：

```sh
"$GODOT_BIN" --headless --path demos/godot --script res://tests/smoke_test.gd
mkdir -p demos/godot/artifacts/benchmarks
"$GODOT_BIN" --path demos/godot --script res://tests/batch_render_test.gd
```

实际 `GODOT_BIN` 为 `/Applications/Godot_mono.app/Contents/MacOS/Godot`。

- `SMOKE_TEST_OK`：加载模型，7 个 motion、8 个 expression，原有控件可调用。
- `BATCH_RENDER_TEST_OK`：默认状态及 8 个 expression，原有两种渲染模式的采样差异均为 0。
- 日志：`target/kasane/cubism-smoke.log`、`target/kasane/cubism-render.log`。

这里验证的是已部署的现有插件仍能工作，没有重新发布或改造 Cubism runtime。

## 实现取舍与后续边界

- Stage 00 的“模型”具体落实为单个 `KasaneMeshView`，不承担参数与变形器求值。
- 同步接口复制输入；暂不做零复制、共享内存或后台求值。
- Document 增加简单 revision 便于判定变化，没有实现事务历史或冲突协议。
- 接口使用单操作完整性，Stage 02 才扩展为跨操作事务、Undo/Redo。
- Godot 源位置快照可以与渲染缓存比较，但不允许通过借用的预览节点反向修改 Document。
- 纹理导入、资源热更新、文件保存、Python SDK、人工拖点都未进入本次范围。
- PNG 是验证输出，不是模型输入或编辑链路的一部分。

下一步可直接以 [路线图阶段 02](../ROADMAP.md) 为基础编写最小修改与事务实施文档。
