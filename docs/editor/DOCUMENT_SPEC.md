# Document specification v1（Stage 01）

状态：已实现的**内存逻辑规格**，不定义磁盘格式或稳定的跨进程协议。

实现入口：`modules/kasane-core/include/kasane/document.hpp`。Godot 调用示例：`demos/kasane-preview/fixture.gd`。

## 1. 权威数据与所有权

Document 是源数据唯一所有者；通过方法创建或修改对象。核心不依赖 Godot、Cubism、GPU 或文件系统。

核心 `get_mesh()`、`get_asset()` 返回只读借用指针，调用方应只在下一次文档修改之前使用，不能绕过 API 写入。创建 Mesh 接受值，内部拥有其副本。Godot Bridge 查询返回独立 packed arrays 和字典，修改快照不修改 Document。

Document 单线程使用，无内部锁。Bridge 的读写在 Godot 主线程进行。revision 表示运行期间的源数据版本，不是持久化版本，也还不是冲突检测协议。

## 2. Document

| 字段 | 类型 | 约定 |
|---|---|---|
| `schema_version` | uint32 | 常量 1，逻辑 schema 标识 |
| `id` | UUID 字符串 | 显式初始化，之后不可修改 |
| `canvas.width/height` | float32 | 有限且大于零，原画逻辑尺寸 |
| `assets` | ID → ImageAsset | 无图像像素和 GPU 引用 |
| `meshes` | ID → Mesh | 源网格 |
| `mesh_order` | Mesh ID 数组 | 创建顺序，稳定遍历及简单绘制顺序 |
| `revision` | uint64 | 初始 0，有效且实际改变数据的操作加 1 |

初始化失败保持未初始化状态；初始化成功后不允许再次初始化。要开启另一文档，创建另一个实例。没有删除、重排或修改画布的公共操作。

内部 `VertexId → dense slot` 索引是可推导的查找缓存，不是第二份拓扑权威数据。创建时建立，位置更新复用；不需要每次改点重新扫描完整拓扑。

## 3. 身份

- Document、Asset、Mesh ID 使用非全零、小写、带连字符的 UUID 字符串，格式为 `xxxxxxxx-xxxx-xxxx-xxxx-xxxxxxxxxxxx`。
- 当前仅校验规范文本形状和非零，不限制 UUID version/variant；生成由调用方负责。
- 文档 ID、资源 ID、网格 ID 在同一文档内不能冲突。
- 名称可重复、可为空，不用于建立对象引用。只实现 Mesh 重命名。
- Vertex ID 是 Mesh 内唯一的 uint32，允许 0；不同 Mesh 可以使用相同 Vertex ID。
- 顶点 ID 不等于数组下标。此阶段不支持增删顶点或复用已删除的 ID。

## 4. ImageAsset

| 字段 | 类型 | 约定 |
|---|---|---|
| `id` | UUID | 稳定身份 |
| `name` | string | 显示名称 |
| `source` | 非空 string | 资源定位标识，如 `memory://quadrants`；核心不解析或读取 |
| `width/height` | uint32 | 非零像素尺寸 |

Bridge 登记一个实际 Texture2D，并把尺寸写入描述；Texture2D 引用留在 Bridge。固定样例在内存中生成图片，不依赖素材导入。

内容哈希、原画图层偏移、热重载和资源导入器尚未实现。`source` 的磁盘解析规则由保存和导入阶段定义。

## 5. Mesh

| 字段 | 类型 | 约定 |
|---|---|---|
| `id` | UUID | 稳定身份 |
| `name` | string | 显示名称 |
| `texture_asset_id` | UUID | 必须引用已有 ImageAsset |
| `vertex_ids` | uint32[N] | Mesh 内唯一 |
| `base_positions` | Vec2[N] | float32，基础形态，模型坐标 |
| `uvs` | Vec2[N] | float32，相对当前纹理 |
| `triangles` | VertexId[T][3] | 引用稳定顶点 ID |

N 至少为 3，T 至少为 1；位置、UV 与顶点 ID 必须一一对应。单个三角形的三个 ID 必须不同且都存在。数值必须有限；不要求网格连通、闭合或无重叠。允许零面积几何和越界 UV，此阶段不为它们产生诊断。渲染纹理禁用 repeat。

渲染接口使用有符号 32 位数组下标，所以顶点数量及三角形展开后的索引数量不得超过 INT32_MAX。完整图结构、Edge 对象、顶点属性系统及非流形校验尚未实现。

## 6. 坐标与 UV

- 模型原点位于原画画布中心；X 向右、Y 向上。
- 单位为原画像素逻辑单位；视口缩放和以后纹理重新采样不自动改变源位置。
- 图像坐标从左上角开始，X 向右、Y 向下。
- UV `(0,0)` 对应纹理左上角，`(1,1)` 对应右下角；UV 不随顶点位置更新自动改变。
- 原画坐标到模型坐标：`x = image_x - width/2`，`y = height/2 - image_y`。
- Godot 渲染时统一转换为 `(x, -y)`。Bridge 的 Node2D 变换只影响预览位置，不写入 Document。
- 不限制三角形 winding；当前二维绘制不以它定义正反面。

## 7. 修改语义

核心提供 `add_asset`、`create_mesh`、`rename_mesh`、`set_vertex_positions`。创建 Asset 必须先于引用它的 Mesh。

`set_vertex_positions(mesh_id, vertex_ids, positions)`：

1. 目标 Mesh 必须存在，ID 与位置数量相同。
2. 每个 ID 必须存在于该 Mesh；同一批次不能重复写同一个顶点。
3. 完整输入校验通过后才写入，不能先写前几个有效顶点再发现后面失败。
4. 未列出的顶点不改变；UV 和拓扑不改变。
5. 空批次或与当前值完全相同的写入是 no-op，不推进 revision。

重命名为原名称也是 no-op。此阶段保障**单个 API 操作**的完整性，不提供多个操作的事务、撤销或重做。正常内存分配失败不属于可恢复编辑错误协议。

## 8. ChangeSet 与错误

成功或失败的编辑返回 `EditResult {status, changes}`。ChangeSet 包含当前 revision、变化种类以及受影响 Mesh ID：

| kind | 含义 | 预览行为 |
|---|---|---|
| none | 无变化或失败 | 不更新 |
| metadata | 名称或资源描述变化 | 不上传几何 |
| positions | 基础位置变化 | 更新目标网格位置 |
| structure | 创建 Mesh | 创建预览及 ID→下标转换 |

失败不会增加 revision，也不产生非空变更集合。主要错误包括 `NOT_INITIALIZED`、`ALREADY_INITIALIZED`、`INVALID_ID`、`DUPLICATE_ID`、`INVALID_CANVAS`、`INVALID_ASSET`、`MISSING_ASSET`、`MISSING_MESH`、`MISSING_VERTEX`、`DUPLICATE_VERTEX`、`INVALID_LENGTH`、`NON_FINITE`、`REPEATED_VERTEX`。

Bridge 还校验 `INVALID_FIELD`、`INVALID_VERTEX_ID`、`INVALID_TEXTURE` 与 `WRONG_THREAD`。调用方应判断 `ok` 和 code，不依赖具体英文 message 文案。

渲染同步与源数据提交是不同结果；Bridge 的 `preview_ok` 及重建行为见 [架构说明](ARCHITECTURE.md)。

## 9. Godot 描述格式示例

这是 API 输入示例，不是磁盘序列化格式：

```gdscript
{
    "id": "33333333-3333-4333-8333-333333333333",
    "name": "Quad",
    "texture_asset_id": "22222222-2222-4222-8222-222222222222",
    "vertex_ids": PackedInt64Array([40, 10, 90, 20]),
    "base_positions": PackedVector2Array([
        Vector2(-80, 80), Vector2(-80, -80),
        Vector2(80, -80), Vector2(80, 80)
    ]),
    "uvs": PackedVector2Array([
        Vector2(0, 0), Vector2(0, 1), Vector2(1, 1), Vector2(1, 0)
    ]),
    "triangles": PackedInt64Array([40, 10, 90, 40, 90, 20])
}
```

Godot 侧用 PackedInt64Array 承载全部 uint32 ID；负值或超过 UINT32_MAX 的值被拒绝，不做截断。核心转换后的渲染索引是 `[0,1,2,0,2,3]`，仅在新建或重建预览时生成。

## 10. 下一阶段的扩展边界

Stage 02 在现有修改入口上增加事务和 Undo；Stage 03 再定义项目文件。Deformer、Parameter、Keyform、预览参数和求值结果不提前塞入当前 Mesh。未来求值输出替换 Bridge 的位置来源，Stage 00 仍接受最终可渲染数组。
