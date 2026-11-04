# Native project format P4 validation

日期：2026-11-04。macOS 27.0.1 arm64，Rust 1.99.0，release 构建。
基线：`a8af647f` 加本阶段实现。原始结果见
[JSON 记录](2026-11-04-native-project-format-p4.json)。受许可限制的模型只在本地读取，未纳入提交。

## 真实工程成本

| 样本 | Mesh / Binding / Motion | JSON / 归档 MiB | 打开 ms | 普通保存 ms | 另存为 ms | 进程峰值 RSS MiB |
| --- | --- | --- | --- | --- | --- | --- |
| Mao | 260 / 185 / 7 | 3.43 / 9.53 | 257–261 | 182–185 | 172–178 | 74.5 |
| 大肥鱼 | 715 / 30 / 0 | 45.83 / 53.63 | 2132–2217 | 1055–1067 | 1008–1197 | 739.1 |

每项运行三次，未清空 OS 缓存。RSS 来自 `/usr/bin/time -l`，涵盖整个进程的设置、
codec 测量和同时存活的验证 Session，不能当作单次打开的净内存。
每轮另存为都比较完整 authoring 内容及每个有效资源的原始 PNG 字节。
Mao 保留全部七段动画；导入器原有的未解析 Parameter/Part 诊断写入 JSON 记录。

本机后续性能回归的参考预算为 Mao 普通保存 1 秒、大肥鱼 3 秒；这不是跨机器 SLA。
当前结果支持首版完整 ZIP 保存，但不能据此推断 16 GiB 工程的响应时间。
容器限额仍独立于宿主的可用内存和共享磁盘预算。

打开时工作目录的逻辑峰值为归档副本加展开 PNG：Mao 19,051,208 字节，
大肥鱼 97,574,704 字节。保存同时需要旧目标、完整 staging 和工作集资源。
这些是根据归档和有效资源长度推导的容量下界，不是物理设备 IO、文件系统块分配或
进程全局临时空间的采样结果。P0 五类合成样本与目录格式对照保留在
[P0 记录](2026-11-04-native-project-format-p0.md)。

复现时为每次执行选择新的输出目录：

```sh
cargo build --locked --release -p zipzi-project \
  --example import_model3_project --example project_io_stress
target/release/examples/import_model3_project \
  /absolute/path/to/model.model3.json /absolute/path/to/input.zipzi
/usr/bin/time -l target/release/examples/project_io_stress \
  /absolute/path/to/input.zipzi /absolute/path/to/fresh-output 3
```

## 本阶段补齐的边界

- 移走 `.zipzi`、删除原始 PNG、释放旧工作集后，在独立子进程重新打开，
  核对完整序列化数据、锁定资源、缺失资源声明和 PNG 字节；另一个过时进程的保存冲突。
- 锁定资产允许经过验证的存储路径/hash 重绑定，保留其 authoring 数据和锁状态；
  已缺失内容的锁定资产可以继续保存。
- 保存反馈经过 Project → SDK → Runtime Work → Editor 状态栏，显示准备、写入和完成阶段；
  写入百分比来自未压缩 payload 字节，finalize/hash/sync 单独显示完成阶段。
  finalize/publish 失败不报告已发布，父目录同步失败仍报告发布成功及警告。
- 解码预留依据中央目录、条目数和解压后的 JSON，覆盖索引、解析中间态、候选文档及
  单张 PNG 解码空间；所有 Job 在同一个宿主预算下增长预留，直到 Completion 被处理。
  高压缩率输入不能通过较小的归档长度绕过 admission。预留是保守估算，不是 RSS 硬限制。
- Observation 输入跨 Session 重置与关闭持有资源；实际 GPU 渲染成功后，最后租约释放才清理。
- Observe 检查样本改为三个自包含归档；生成器、SDK agent 样本、压力脚本与例程使用正式入口。

主要验证命令：

```sh
cargo test --locked -p zipzi-project --lib
cargo test --locked -p zipzi-project --test project_tests
cargo test --locked -p zipzi-sdk --test integration project_io::
cargo test --locked -p zipzi-runtime --test runtime
env -u ZIPZI_DEV_SESSION_DIR cargo test --locked -p zipzi-editor-core --test integration runtime::
env -u ZIPZI_DEV_SESSION_DIR cargo test --locked -p zipzi-editor --lib project
env -u ZIPZI_DEV_SESSION_DIR cargo test --locked -p zipzi-editor --lib headless_tests::runtime_host
cargo test --locked -p zipzi-observe --test capture archive_observation_input
./tools/test-sdk.sh python
./tools/test-sdk.sh python-connected
./tools/dev-python fixtures/observe_inspection/build_fixtures.py --verify
uv run --locked python tools/test_tools.py -p test_acceptance_validators.py
uv run --locked python tools/test_tools.py -p test_python_codegen.py
```

## 待平台验收

Windows 原生构建、用户级注册/卸载关联、占用句柄替换、no-replace 和原生对话框，
以及两平台冷启动/运行中/无窗口/启动中通过系统双击打开文件，仍需实机逐项执行。
macOS 编译、GPU 观察与队列/IPC/生命周期逻辑测试不能代替这些交互验收。
因此 P4 的跨平台最终退出条件尚未满足。
