# C6 核心创作性能复核

2026 年 10 月 22 日；对应[核心总验收](../plans/unified-object-authoring-c6-acceptance.md)。
环境：Apple M4（Mac16,10）、16 GiB、macOS 27.0.1（26A434）、Rust 1.99.0。
授权模型不纳入仓库。本轮使用 Mao 和 Shirousagi 的实际 model3 输入；原始输出保留在
`target/`，不修改模型文件。

## 方法与边界

- 属性链沿用 `scripts/property-chain-benchmark.py` 的 plain／instrumented Release
  两种构建，Mao、retained 旧索引读者、每进程 10 次预热／60 次手势、每次 12 次更新、
  三轮交替进程顺序。plain 衡量总耗时，instrumented 检查阶段与工作量。
  该配对用于测量仪表开销，不是旧版本／新版本配对。既有历史记录仅作同机量级参考。
- 新增 SDK `c6_authoring_benchmark`：能力查询、注册 hover 移动规划、有使用者纹理
  的删除拒绝各 4 次预热＋128 次采样；注册移动、空参数组创建、纹理记录复制的
  规划／提交／Undo 分别 4 次预热＋32 次采样。所有读取验证版本、求值版本、历史与
  事件不变；每个实际结构操作验证一条历史，并恢复初态。探针创建的临时对象仅存于会话。
- C4 探针沿用真实模型前三个最大普通 Mesh 网格、八次插点转换／Undo，比较默认
  预算估算与实际分配峰值，验证网格工作中的协作取消及历史不变。
- SDK 两个探针共用 `System` 分配器包装，报告每阶段相对基线的最大 live allocation
  增量。提交基线含计划，Undo 基线含计划／回执；阶段峰值不能相加。不计分配器保留页、
  栈、GPU 或文件映射，不是 RSS。C4 临时预算结论沿用同一测量定义。
- Timeline 使用现有真实 GPUI 输入、CPU／Metal 路径、Natural／Dense 变体与保存往返。
  原生测量不含 compositor 呈现；属性链 CPU 探针不测 FPS 或输入到屏幕延迟。
- 先完成构建／普通测试，再顺序测量，避免本任务的并发编译干扰；普通系统后台活动
  仍可能影响尾延迟。属性链 runner 另检查边界上的编译活动、来源和模型哈希。

```sh
# Editor 根目录
python3 scripts/property-chain-benchmark.py \
  models/local/mao/runtime/mao_pro.model3.json target/c6-property-retained --reader-policy retained
./scripts/timeline-benchmark.sh target/c6-timeline 120

# 相邻 kasane-2d 根目录；各命令顺序执行
cargo build --locked --release -p kasane-sdk \
  --example c6_authoring_benchmark --example c4_authoring_benchmark
target/release/examples/c6_authoring_benchmark /absolute/path/to/model.model3.json
target/release/examples/c4_authoring_benchmark /absolute/path/to/model.model3.json
```

## 属性链

下表单位 ms，取三个 plain 进程中位数的中位数；P95 列取三个进程各自经验 P95 的
中位数。更新＋预览由同一次输入的两个阶段先配对相加，再计算分位数。

| 负载 | 更新 p50 | 更新＋CPU 预览 p50 | 更新＋CPU 预览 P95 |
| --- | ---: | ---: | ---: |
| Mesh Opacity | 0.176 | 0.291 | 0.858 |
| Mesh Multiply | 0.177 | 0.292 | 0.866 |
| Parameter Default | 0.324 | 0.453 | 0.954 |
| Motion Fade | 0.005 | 2.955 | 3.685 |
| Expression Fade | 0.011 | 3.000 | 3.757 |
| Physics Mobility | 0.027 | 3.221 | 3.891 |
| Timeline Value | 0.420 | 0.663 | 0.957 |

全部六个进程完成真实修改、候选独立、取消恢复、一次 finish 一条历史、Undo 和
CPU 预览校验，runner 的覆盖、配对、来源与模型哈希检查通过。instrumented 三轮
均验证：Mesh Opacity 更新没有对象记录复制；Motion Fade 每次调用一次
`core.patch_motion_properties`、复制一个索引记录，无完整 Motion 校验；Physics
每次复制两个记录（Group 与稳定 Node），不复制整棵索引或画布拓扑。

与[前轮记录](2026-10-21-property-chain-refactor.md)的小样本 retained 结果相比，
相同三条热链的更新＋预览为 0.321→0.291、3.682→2.955、3.843→3.221 ms；
Timeline 为 0.640→0.663 ms。采样量与源码已变化，这些数值只说明量级延续，
不作为版本配对提速或尾延迟结论。完整属性提交的 p50 仍约 13–14.4 ms，带预览的
Motion／Expression／Physics 提交约 17 ms；交互结束的完整校验／历史发布成本仍存在。

原始样本、阶段归因、每进程分位数、两个二进制及其哈希、来源归档和模型子树哈希：
`target/c6-property-retained/`。本轮未另跑 unique-reader 控制组。

## SDK 读取与结构操作

查询采用预热后的稳定文档。耗时为单个查询的 p50／P95，分配是该查询的最大额外
live 字节；不会推进版本／求值版本、创建历史或发布事件。

| 查询 | Mao p50／P95 ms | Mao 峰值 B | Shirousagi p50／P95 ms | Shirousagi 峰值 B |
| --- | ---: | ---: | ---: | ---: |
| Mesh capabilities | 0.061／0.067 | 432 | 0.006／0.006 | 432 |
| 注册移动 hover plan | 0.121／0.127 | 62,328 | 0.005／0.005 | 14,325 |
| 有使用者纹理删除拒绝计划 | 0.053／0.058 | 54,140 | 0.004／0.004 | 5,135 |

结构计划在每次 Undo 后重新取得，包含恢复导致的只读缓存重建；它们不等同于上表的
稳定 hover 查询。每行分别列规划／提交／Undo 的 p50 和各阶段峰值，不能相加分位数
或内存峰值。两模型都验证一动作一历史、Undo 恢复且只发布两次通知。

| 模型／操作 | 规划／提交／Undo p50 ms | 规划／提交／Undo 峰值 B |
| --- | ---: | ---: |
| Mao 注册移动 | 0.673／13.920／1.595 | 1,514,560／8,919,715／1,906,676 |
| Mao 空 ParameterGroup 创建 | 0.316／12.933／1.638 | 782,924／9,166,383／1,629,672 |
| Mao 纹理记录复制 | 8.536／13.815／1.811 | 5,114,924／9,163,305／2,646,028 |
| Shirousagi 注册移动 | 0.006／0.600／0.071 | 15,029／699,309／69,712 |
| Shirousagi 空 ParameterGroup 创建 | 0.011／0.604／0.074 | 8,194／733,489／35,796 |
| Shirousagi 纹理记录复制 | 0.317／0.588／0.087 | 384,755／732,054／121,632 |

Mao 结构提交观测最大值为 15.285 ms；纹理复制规划仍明显重于 hover 查询。
这些完整结构动作按需执行，没有将其放入 hover／每帧能力查询，也没有用整个对象
序列化实现查询。原始输出与两个 SDK 二进制哈希：`target/c6-sdk/`；对应源码归档
见属性链目录。上述结构负载没有旧版本二进制配对，不声明速度提升。

## Binding 容量复核

每行覆盖三个绑定；耗时是各绑定八次测量的 p50 范围。

| 模型 | 组合数 | 规划／提交 p50 ms | 规划峰值 B | 提交峰值 B | 临时估算 B | 历史估算 B |
| --- | ---: | ---: | ---: | ---: | ---: | ---: |
| Mao | 12 | 0.407–0.412／13.718–13.795 | 874,068–878,004 | 8,888,294–9,166,210 | 16,099,654–16,115,398 | 5,411,842–5,419,714 |
| Shirousagi | 3–4 | 0.017–0.020／0.626–0.638 | 19,508–25,060 | 697,684–731,944 | 1,481,540–1,499,300 | 506,076–514,956 |

两个模型的规划与提交分配峰值均小于计划估算，默认 65,536／64 MiB／256 MiB
预算通过。协作取消进入网格工作后返回，版本与历史不变；释放错误与控制 token 后，
额外 live 字节为 Mao 0、Shirousagi 200，含只读缓存记账，不是可提交候选。
与前次[容量测量](2026-10-22-binding-authoring.md)采用相同定义，仍不将估算解释为 RSS
或任意模型的严格分配上限。原始输出：`target/c6-sdk/c4-*.log`。

## Timeline

现有 Release 原生验收通过。Natural 为 134 tracks／779 keys，Dense 为
134 tracks／34,438 keys。滚动、突发请求、播放各 120 次；拖动和 seek 各 32 次。
下表为本轮单进程的 p50／经验 P95，单位 ms。

| 阶段 | Natural p50／P95 | Dense p50／P95 |
| --- | ---: | ---: |
| 滚动 dispatch＋draw | 3.909／5.203 | 7.871／9.602 |
| 拖动 dispatch | 4.399／4.524 | 26.592／27.360 |
| 拖动到当前 Metal draw | 20.019／20.632 | 74.081／75.526 |
| Seek 到当前 Metal draw | 9.364／19.069 | 9.089／21.469 |

两个变体各观察到 119 个播放帧，最大时钟滞后约 0.01667 s；静态曲线 scene rebuild
和 repaint 均为 0。候选取消像素恢复、一次拖动一次 Undo、过期结果拒绝、SDK
数值／几何对照及保存重开不变均通过。画布导航与两工作区右侧 Dock resize 同样通过。

原生 Dense 拖动的 74 ms 端到当前 Metal draw 延迟仍是实际成本；C6 不将其当作
60 FPS 保证。该原生测试含更密集的 34,438-key 数据与 UI／异步／GPU 阶段，不能用
上文单值 CPU Timeline 的 0.663 ms 代替。没有同源码的旧原生二进制配对，不据此
声明原生拖动提速。数据、环境与运行日志保留在 `target/c6-timeline/`。

## 验收结论

本轮通过真实模型的行为、容量、协作取消与缓存工作量门禁，固定有限默认预算。
属性热链保持已有测量的量级，结构操作与原生 Timeline 的完整成本已单独记录。
该结论支持 C6 核心交付；密集 Timeline 的原生延迟、完整提交成本与更大规模模型
仍可继续优化，不扩大当前测量覆盖范围。
