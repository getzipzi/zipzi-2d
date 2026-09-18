# Kasane project format v1（Stage 03）

项目是 UTF-8 JSON 文件，顶层 `format` 固定为 `kasane-project`，`format_version` 当前为 `1`。未知版本必须以 `UNSUPPORTED_VERSION` 拒绝，不做猜测性加载。

`document` 保存 Document ID、`canvas`、有序的 `assets` 和 `meshes`。Asset 保存稳定 ID、名称、资源 `source` 及预期像素尺寸；Mesh 保存稳定 ID、纹理 Asset 引用、Vertex ID、基础坐标、UV 和以 Vertex ID 表示的三角形。`revision`、Undo/Redo 历史、预览节点、GPU 句柄和纹理像素不写入项目。

资源规则：

- `source` 是 Godot 可解析的资源路径，例如 `res://assets/quadrants.svg`。
- 打开项目时先解析到临时 Document，再解析所有纹理并核对保存的宽高。
- 资源不存在返回 `MISSING_RESOURCE`，尺寸变化返回 `RESOURCE_MISMATCH`；失败不会替换当前打开的 Document。
- 打开成功后从源数据重建稠密索引和全部预览资源，文档处于未修改状态且没有跨会话 Undo 历史。

保存规则：

- JSON 先完整写入目标文件同目录的临时文件，再通过文件系统 rename 替换目标。
- 临时文件创建或替换失败返回 `SAVE_FAILED`，不会先截断旧项目。
- 活跃事务不能保存；必须先提交或取消。
- 保存成功才更新内存保存点。Undo 回到该保存点时 `modified` 会恢复为 false。

可重开样例位于 `demos/kasane-preview/samples/stage-03.kasane.json`。
