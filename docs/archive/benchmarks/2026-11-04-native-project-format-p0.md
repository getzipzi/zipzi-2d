# Native project format P0 storage prototype

日期：2026-11-04。平台：macOS 27.0.1，arm64。提交基线：`codex/native-project-format`。

这次 P0 原型放在 `zipzi-project` 的 `archive-prototype` feature 下，使用 lockfile 中的
`zip` 9.0.3。它只读写带有 `prototype-directory-document-v21` required feature 的实验归档，
不会改变当前 v21 目录工程的正式入口，也不会迁移历史工程。

## 复现

```sh
cargo build --locked --release -p zipzi-project \
  --features archive-prototype --example project_archive_prototype
uv run --locked python tools/benchmarks/project-archive-prototype.py \
  --output target/archive-p0 \
  --sample real-cat=/absolute/path/to/saved-generated-cat
```

脚本在一个新的输出目录生成五类合成样本，并对每个样本测量目录与归档的打开、保存和
另存为。`target/archive-p0/report.json` 保存了本次运行的完整 JSON。每次归档写入使用
1 MiB 缓冲；PNG 使用 Stored，JSON 使用 Deflate；ZIP 条目固定顺序、时间戳和权限，并强制
写入 ZIP64 头。写入完成后再次读取已 finalize 的文件计算整体 SHA-256 fingerprint。

## 样本与结果

| 样本 | 文档字节 | 唯一 PNG / PNG 字节 | 归档字节 | 目录打开 ms | 归档打开 ms | 目录保存 ms | 归档保存 ms |
| --- | ---: | ---: | ---: | ---: | ---: | ---: | ---: |
| empty | 928 | 0 / 0 | 886 | 0.090–0.202 | 0.219–0.487 | 11.700–11.825 | 12.486–12.735 |
| small-images | 68,661 | 256 / 1,074,176 | 1,155,954 | 33.514–36.041 | 68.338–73.229 | 49.713–66.601 | 73.428–79.386 |
| large-texture | 1,194 | 1 / 16,780,612 | 16,781,874 | 7.940–12.834 | 32.677–35.911 | 30.805–35.755 | 56.304–65.933 |
| heavy-keyforms | 7,208,499 | 1 / 16,516 | 1,260,568 | 67.187–72.537 | 75.538–78.616 | 37.857–39.216 | 98.952–102.725 |
| missing | 1,016 | 0 / 0 | 922 | 0.096–0.207 | 0.219–0.487 | 11.785–11.832 | 12.511–12.623 |
| real-cat | 219,713 | 24 / 1,473,319 | 1,571,228 | 16.288–22.835 | 28.575–29.373 | 18.845–20.649 | 26.214–31.655 |

时间是每个子进程的三次运行范围，包含该命令的设置和验证；没有清空 OS 缓存，不能当作
产品 SLA。逻辑归档 IO、RSS、块 IO 与临时空间峰值在 JSON 中分别记录；物理设备流量尚未
测量。大纹理样本的归档打开额外读取归档副本、展开 PNG 并逐张解码，保存过程不会为打包
目的批量解码 RGBA。

## 已验证的边界

Rust 聚焦测试覆盖完整 SHA-256 资源去重、缺失资源、package attachment、文档和资源字节
往返、ZIP64/Stored/Deflate、取消、短写、输入增长、截断、重复和路径逃逸、未知条目、
加密/符号链接标记、压缩方法、CRC、重叠条目、限额失败以及私有工作集租约。测试命令：

```sh
cargo test --locked -p zipzi-project --lib archive:: --features archive-prototype
cargo test --locked -p zipzi-project --lib filesystem::tests -- --nocapture
uv run --locked python tools/workspace/cargo_domain.py --check
```

当前实验读取器仍将 ZIP 中央目录交给 `ZipArchive` 后才做全部 admission；P1 需要把归档
长度、中央目录长度和条目数量检查前移，并增加本地头/中央目录边界一致性校验。P1 还要将
工作集、发布事务、替换/不替换原语和 fingerprint 接入正式 Project API，再开始 SDK、Runtime
和 Editor 生命周期改造。
