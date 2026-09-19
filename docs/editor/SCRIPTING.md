# Stage 04：应用内脚本编辑

从仓库根目录构建和验证：

```sh
python3 tools/run_kasane_preview.py --render
python3 tools/run_kasane_preview.py --skip-build --demo
```

演示底部输入 GDScript 文件路径，点击 Run script 或按 F5。修改脚本后再次执行会读取最新代码，操作当前内存 Document，不重新加载模型。也可在启动时指定脚本：

```sh
/Applications/Godot_mono.app/Contents/MacOS/Godot --path demos/kasane-preview -- --edit-script=res://scripts/edit_document.gd
```

文件继承 `RefCounted`，实现同步的 `run(ctx)`，返回结果 Dictionary。`ctx.document` 是当前 Document Bridge，`ctx.actions` 是可选 Action 服务。

```gdscript
extends RefCounted
func run(ctx) -> Dictionary:
    var id = ctx.document.get_document_summary().meshes[0].id
    var mesh = ctx.document.get_mesh(id)
    mesh.name = "Edited mesh"
    var positions = mesh.positions
    positions[0] += Vector2(0, 10)
    mesh.positions = positions
    return {"ok": true, "mesh": mesh.snapshot()}
```

这段代码直接修改数据，不创建历史，不要求事务。数组需要显式写回。脚本出错时此前已完成的直接写入保留。需要可编程错误结果时使用 `mesh.set_vertex_positions(ids, positions)`，不要仅依赖属性赋值的日志。

需要一组修改可撤销时，参见 `scripts/action_edit.gd` 的 `ctx.actions.perform(label, callable)`。回调显式返回 `{"ok": true}` 才登记历史；失败则恢复文档。演示提供 Undo action / Redo 按钮。也可以直接使用 Godot `UndoRedo` 配合网格对象属性。

Action 辅助类使用整个文档快照，撤销可能覆盖后续直接写入；原生属性 Action 则只恢复登记的属性。两者都不是自动追踪任意内存变化。详见 [架构与边界](ARCHITECTURE.md)。

检查接口：`get_document_summary()`、`mesh.snapshot()`、`get_mesh_view(id).get_positions_snapshot()`。Run/F5 在有真实渲染时输出 `artifacts/script-preview.png` 和对应 revision；无渲染环境不会生成伪截图。

普通脚本可用 `ctx.document.create_mesh(description)` 创建网格，或用 `mesh.replace_geometry(ids, positions, uvs, triangles)` 直接替换几何；均不必经过 Action。保存使用 `save_project(path)`，打开使用 `open_project(path)`。打开后应重新获取网格句柄。

当前没有脚本超时隔离、后台执行或 CPython。脚本运行于主线程，必须短小并正常返回；不支持异步 `run`。脚本宿主不自动保证全部 setter 成功，脚本应检查方法结果并返回诊断。

## Stage 05：Rotation 与 Warp

运行演示后，将路径设为 `res://scripts/stage05_deformers.gd` 并 Run/F5；或启动时指定：

```sh
/Applications/Godot_mono.app/Contents/MacOS/Godot --path demos/kasane-preview -- --edit-script=res://scripts/stage05_deformers.gd
```

脚本创建 25 顶点网格、2×2 Warp 和 Rotation，绑定成 Mesh → Warp → Rotation，并直接修改中心控制点、上边控制点与旋转角度。源顶点仍保持规则网格，右侧画布显示求值后形态。

- `document.create_rotation(id, name, center, angle_degrees)`、`create_warp(id, name, origin, size, columns, rows)` 创建变形器。
- `document.get_deformer(id)` 返回句柄；Rotation 可直接设置 `center`、`angle_degrees`，Warp 可读写 `control_points`（数组需写回）。
- 需要结构化结果时使用 `update_rotation(center, angle)`、`update_control_points(points)`。
- `document.set_deform_parent(child_id, parent_id)` 或变形器的 `bind_to(parent_id)` 设置变形父节点；空字符串解除绑定。
- `document.set_organization_parent(child_id, parent_id)` 单独组织对象，不影响画面。
- `document.evaluate_mesh(mesh_id)` 返回只读求值位置和 revision；`mesh.positions` 始终是源数据。
- `ctx.actions.perform("Deform part", callback)` 可将创建、绑定、控制点修改与旋转组合成一个可撤销 Action；普通写入不自动记录历史。

可重开项目：`res://samples/stage-05.kasane.json`。坐标、插值、边界外推与嵌套限制见 [Stage 05 约定](stages/05-deformers.md)。保存现在写 v2，旧 v1 仍可读取。
