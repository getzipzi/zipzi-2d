# Stage 02–03 交付与验收记录

日期：2026-09-18。状态：Stage 02、Stage 03 在 macOS arm64 / Godot 4.7.2 下已验收。

## Stage 02：修改与事务

- 核心提供 `begin_transaction`、`stage_vertex_positions`、`commit_transaction`、`cancel_transaction`，以及一次调用提交多组显式 Mesh/Vertex ID 更新的入口。
- Commit 前验证整批命令；缺少 Mesh/Vertex、重复写同一顶点、长度错误和非有限坐标均不会产生部分写入或 revision。
- 一次非空 Commit 只增加一个 revision 和一个 Undo 步骤；Undo/Redo 同步返回 ChangeSet，Bridge 沿用 Stage 01 的预览更新通路。
- 未提交事务只持有命令副本，取消不修改 Document。新提交会清空 redo 分支；内存历史上限为 128 步，不跨会话保存。

## Stage 03：保存与重开

- `KasaneDocumentBridge.save_project(path)` 安全替换 JSON v1 项目；`open_project(path)` 先完整验证临时 Document 和纹理资源，成功后才替换当前文档。
- 保存点与运行时 revision 分离；成功保存或打开后 `modified == false`，编辑后为 true，Undo 回保存点可恢复 false。
- 打开时重建稳定 Vertex ID 索引、稠密渲染索引与 GPU 预览。项目不保存派生缓存或历史。
- 已覆盖不支持版本、资源缺失、资源尺寸变化和无效项目的结构化错误。格式详见 [项目格式 v1](../PROJECT_FORMAT.md)。

## 验收

从仓库根目录运行：

```sh
python3 tools/run_kasane_preview.py --render
```

核心 CTest 覆盖原子事务、取消、Undo/Redo、redo 分支失效和保存点脏状态。Godot 集成测试覆盖固定命令的源数据/预览一致性、一次历史步骤、重复安全替换、关闭式新 Bridge 重开、ID/坐标/UV/拓扑/纹理关系保持、失败打开保留当前文档，以及仓库内样例重开。GPU 测试继续验证实际纹理方向与修改后的画面。

Stage 03 不包含文件对话框、自动保存、压缩包、迁移框架、跨会话历史或发布格式。
