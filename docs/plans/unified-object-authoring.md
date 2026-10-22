# 对象创建编辑分组与排序方案索引

日期：2026 年 10 月 21 日，进度更新于 10 月 22 日。核心 C1–C6 已完成并交付；全面 UI 统一尚未开始。详见 [C6 核心交付](unified-object-authoring-c6-acceptance.md)、[核心实施记录](unified-object-authoring-core-implementation.md)与 [C5 覆盖矩阵](unified-object-authoring-c5-coverage.md)。

原统一方案已拆成两份，按以下顺序实施。本文件只保留导航和范围边界，不再维护第三份方案正文。

| 顺序 | 方案 | 负责范围 |
| --- | --- | --- |
| 1 | [对象创作核心架构改造](unified-object-authoring-core.md) | 全部对象的身份与集合、原生数据模型、SDK/editor-core 操作契约、事务、执行与格式、核心验收 |
| 2 | [Editor UI 交互统一](unified-object-authoring-ui.md) | 创建入口、Properties、文件夹、选择与预览、拖拽、键盘焦点、反馈及 UI 验收 |

核心阶段先完成可独立测试的领域能力，并提供让现有 Editor 继续运行的必要适配；UI 阶段再全面接入统一交互。新 UI 不自行补写领域规则。

已确认的约束在两份方案中保留：New 直接创建后在 Properties 编辑；Motion/Expression 单击保持现有预览；Sort 覆盖同级排序、跨组移动与改父级；支持 Group 的对象能创建空组。所有先前列出的对象和内部条目仍在范围内。

完整能力清单与模型改动以核心方案第 4、5 节为准。影响数据与领域语义的待讨论选择在核心方案第 12 节，纯 UI 选择在交互方案第 10 节。
