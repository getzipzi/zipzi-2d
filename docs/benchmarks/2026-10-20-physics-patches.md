# Physics 字段 patch：实现与测量

物理字段编辑现在通过通用 `PropertyPatch → Gesture::Properties → EditSession::patch_physics`。
名称、FPS、输入/输出映射、权重、缩放、标准化与摆段属性的写入规则集中在
`editor-core/src/property/physics.rs`，UI 只定义字段布局与显示名称。
摆段设置复用也是一次多字段 patch。

SDK 的 `PhysicsPatch` 使用类型化字段，定位 asset、group 和数组元素。校验通过后
原子更新目标组；失败保留候选内容。物理 asset 与各组采用 `Arc` 共享，旧快照与撤销
历史保持独立，未编辑组的指针保持共享。参数重绑定使用稳定 wire key，避免参数改名
后重绑定一个组时影响其他组。

拖拽期间复用已有候选事务，不再为每次字段输入重新创建候选文档。属性索引局部刷新，
画布拓扑保持复用。拖拽结束提交一条历史，取消及返回原值不会创建历史步骤。
导入和增删结构继续使用整个配置的原子提交。

## 写入基准

2026-10-20，本机 Release 构建，三轮结果的中位数：

| 配置 | 整配置 setter | 字段 patch | 加速比约值 |
| --- | ---: | ---: | ---: |
| 16 组 × 4 节点 | 53.36 μs/次 | 4.90 μs/次 | 11× |
| 192 组 × 24 节点 | 2797.23 μs/次 | 23.11 μs/次 | 121× |

每次交替设置一个节点的 Mobility。小配置运行 2000 次，大配置运行 600 次。
setter 路径也使用新的共享存储；patch 路径每次保留一个旧文档读者，强制发生写时复制。
候选创建、最终历史提交、物理重算、模型几何求值和渲染不计入计时。
这些数字说明字段写入成本的改善，不能按同样倍率推断界面帧率。

从编辑器目录复现一轮：

```sh
cargo test --locked --release --manifest-path ../kasane-2d/Cargo.toml -p kasane-sdk --test physics_editing physics_field_patch_benchmark -- --ignored --nocapture
```

## 验证

- SDK/core/project/animation：325 项通过，另有 1 项性能测量默认忽略。
- editor-core：342 项通过，2 项默认忽略。
- 编辑器：339 项通过，1 项默认忽略。全套运行排除了之前已知不稳定的画布缩放通知
  计数用例；该用例随后单独运行通过。物理相关 7 项测试均通过。
- 覆盖字段校验、失败原子性、快照隔离、未修改组共享、撤销/重做、无变化保留 redo、
  参数重命名后的绑定、工程 JSON 编解码、属性索引与画布拓扑复用。
