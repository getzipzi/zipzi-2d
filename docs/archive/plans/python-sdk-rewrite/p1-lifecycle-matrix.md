# P1 生命周期故障矩阵

日期：2026-10-25。基础提交 `7cf4bed` 保存资源/Plan 切片；本表对应其后的工作区实现。源代码验收使用 Release Local 扩展与真实 CPU IPC 宿主，公共类型检查另跑严格正反例。故障控制仅存在于 `test-support` feature，正式 wheel 不包含控制入口。

证据来自 [`tests/test_api.py`](../../../../crates/zipzi-python2/tests/test_api.py)。下表测试名均省略 `PythonSdk.test_` 前缀；每项内部可能覆盖多种故障或两种后端，不按分支重复计数。

| 故障/边界 | Local | Connected | 测试与独立断言 |
| --- | --- | --- | --- |
| 普通领域、编码、非有限值写失败 | 通过 | 通过 | `caught_domain_and_conversion_errors_poison_the_entire_candidate`：捕获错误仍不可提交，无正式部分发布 |
| apply 准入预算 | 通过 | 通过 | `admission_rejection_retains_candidate_and_source`：来源/序号不变，后续可编辑 |
| 已完成但未消费的 Job 占满结果槽 | 通过 | 通过 | `retained_job_budget_refusal_keeps_edit_retryable`：拒绝 commit 不消耗候选，释放一个结果槽后可提交 |
| 宿主交互造成确认 Busy | Rust 宿主门禁 | 通过 | `confirmed_host_busy_commit_keeps_candidate_retryable`：相同候选可重试，只有一步历史；本地队列拒绝另由 Runtime host 单测验证 |
| Busy 拒绝响应丢失后恢复 | 无网络请求表 | 通过 | `recovered_busy_commit_preserves_candidate_for_retry_or_cancel` 及安装 `installed_lost_busy_response_allows_retry_and_cancel`：恢复确认 Busy 后回到可编辑状态，候选来源不变，可继续写入后只提交一步或取消；见[整体验收](../sdk-runtime/r1-r3-python-p1-p2-acceptance.md) |
| 排队 Job 取消 | 通过 | 通过 | `queued_cancel_releases_candidate_and_retains_terminal_error`：终态 CANCELLED，无历史发布，候选图片字节归零，重复等待同一错误 |
| 进入 IO 后取消 | 通过 | 通过 | `cancellation_after_entering_io_reports_actual_save`：实际写出的工程与成功回执一致，不伪造回滚 |
| 实际文件系统失败 | 通过 | 通过 | `real_save_failure_releases_result_and_path_without_publication`：准入后改变目标路径触发 IO 错误；原文件、版本、历史、modified 保持，后续保存可成功 |
| executor 未能执行 | 通过 | 通过 | `worker_failure_and_panic_have_distinct_recovery_scopes`：EXECUTOR_FAILURE，正式数据/历史保留，后续仍能编辑 |
| worker panic | 通过 | 通过 | 同上：WORKER_PANIC，冻结快照仍可读，Session Faulted 拒绝新写 |
| 无后续调用的完成消费 | 通过 | 通过 | `idle_completion_and_close_during_entered_io`，并由 Rust `completion_wait_does_not_pump_runtime_or_require_a_later_getter` 独立证明 owner 完成通知不依赖 getter |
| 本地 close 等待已进入 IO | 通过 | 生命周期不同 | `idle_completion_and_close_during_entered_io`：close 不提前返回，保存结果能被新 Runtime 从磁盘重开 |
| Connected close 释放未提交资源 | 无 IPC client | 通过 | `connected_close_releases_client_leases_and_recovers_lost_ack`：PreparedImage/Plan/候选释放，宿主正式历史保留；close 丢响应原 ID 恢复 |
| Connected close 不取消已进入的任务 | 无 IPC client | 通过 | `connected_close_does_not_cancel_entered_publication`：观察客户端读到实际保存及原历史 |
| 最后一个原生 owner 析构 | 通过 | 宿主由外部拥有 | `last_native_owner_releases_project_lock_and_unsaved_memory`：GC 后后台宿主不再被包装保留，新 Runtime 获得工程锁并读到相同像素 |
| 写入/commit/Job 终态丢响应 | 无网络请求表 | 通过 | 原有恢复用例及 `mixed_resource_batch_response_loss_does_not_repeat_a_write`、`plan_apply_loss_and_foreign_closed_plan_references`：按原 ID 恢复，不重复创建或推进序号 |
| 创建、查询和清理丢响应 | 无网络请求表 | 通过 | 原有生命周期用例及资源/Plan 恢复用例：恢复返回正确公共对象；另一 owner 不能消费 |
| cancel/status 丢响应 | 无网络请求表 | 通过 | `cancel_and_state_response_loss_cannot_be_consumed_by_wait`：等待不能吞掉待确认 cancel/status |
| 失败终态查询丢响应 | 无网络请求表 | 通过 | `failed_terminal_reply_recovery_caches_original_error`：恢复后重复等待保留原始错误，不退化为 INVALID_REFERENCE |
| 同一 owner 的旧异常再次恢复 | 本地无丢响应 | 通过 | `old_exception_cannot_consume_a_later_request_from_the_same_owner`：Session 创建、读取及 Edit 的旧异常均不能消费新请求 |
| 请求结果过期 | 无请求保留表 | 通过 | `expired_request_never_replays_a_write_under_a_new_id`：apply、prepare、plan、commit、job 均拒绝原请求重放，RequestId 不增加；已发布 commit 的历史可读 |
| 客户端 grace 过期 | 显式本地生命周期 | 通过 | `client_expiry_reclaims_prepared_plan_and_candidate_without_new_identity`：真实 Rust 资源字节归零、正式历史不变，不自动注册新 client |
| 宿主暂时离线及重启 | 显式本地生命周期 | 通过 | `host_restart_refuses_original_pending_request`：离线仍 OutcomeUnknown；新实例明确 HOST_RESTARTED，原 RequestId 保留 |

## 资源、来源与安装的配套验收

- `independent_preparation_and_hundred_cancelled_leases_return_to_zero`：两后端各 100 轮独立准备失败、候选/Plan 引用和取消，直接核对底层资源条目及存活字节，不以 RSS 代替。
- `stale_plans_keep_reports_and_abort_without_partial_publication` 与 `formal_plan_rejects_other_session_and_undo_redo_versions`：正式/候选 Plan、错误 Session、候选序号、Undo/Redo 的版本变化均拒绝过期应用，缓存报告仍可读。
- 创建/替换图片、A/B 独立持有、快照、Undo/Redo、单次历史、保存重开由固定像素和几何断言验证；共享 Rust resource/host/protocol 测试另行保护实际持有规则。
- 两个正式 wheel 在互相隔离的纯包连接环境和纯包+原生环境执行 `installed_recipe.py`，覆盖图片、网格、Plan、替换、Undo/Redo、Job 状态/终态取消、保存及新 Runtime/新宿主从磁盘重开。安装验收不使用故障 feature，也不宣称完成 P6 的平台、sdist 或真实 Editor 验收。

## 运行与边界

```sh
./tools/test-sdk.sh python2 --release
# 带 P0 分配器探针的另一份独立产物，不能用于延迟计时。
./tools/test-sdk.sh python2 --release --instrumented
cargo test --locked -p zipzi-runtime --test host --test protocol --test resources --test runtime
cargo test --locked -p zipzi-runtime --features bench-instrumentation --test metrics
```

Job 的 `close()` 仍等待实际结果；`cancel()` 是独立请求，不承诺已经进入的 IO 可回滚。终态成功/失败都会缓存并释放结果槽。未知结果、准入拒绝与执行失败分别处理。回执过期或宿主不可达不能被缓存为“任务失败”：后续 state/cancel 继续报告未能确认结果的原错误，不能把已经发布的提交伪装成失败。

明确关闭带有其他未确认请求的 Connected Runtime 会断开连接，原请求不使用新 ID 重放，其未提交资源由 grace expiry 回收。显式普通关闭通过 `release_client` 释放资源；若确认响应丢失，仍可恢复该关闭请求。正式会话由宿主保留。
