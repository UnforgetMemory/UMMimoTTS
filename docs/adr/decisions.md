# ADR — UM-MimoTTS v4 决策记录

> 完整背景与证据：`docs/compose/plans/2026-08-28-mimotts-workbench-rebuild.md`
> 与 `docs/compose/plans/2026-08-28-mimo-adaptation-research.md`。
> 状态：`Accepted`（2026-08-28 用户确认方向后执行）。

| # | 决策 | 状态 | 落地位置 |
|---|------|------|---------|
| ADR-001 | 后端保留 Rust（tokio + actix-web 4），不做 TS 服务端；TS7/React 仅前端 | Accepted | `crates/*` workspace |
| ADR-002 | 不迁移 axum | Accepted | `crates/mimotts-api`（actix-web 4） |
| ADR-003 | 契约先行：OpenAPI 3.1 单一事实源，`openapi-typescript` 生成前端类型，禁止手写业务类型 | Accepted | `packages/contract/openapi.yaml`；`apps/web/src/api/v3.d.ts`（生成物） |
| ADR-004 | 队列 = 内存优先队列 + SQLite 持久源 + `notify` 唤醒 + 空闲退避 50→500ms + 每 30s 恢复补种 | Accepted | `engine.rs`（Queue/worker/recovery） |
| ADR-005 | 分片：目标 6000 / 硬上限 7500（官方 8K，余量 ≥12%）；单一校准估算器（CJK×2.0/其他×1.2）；风格指令逐片携带；英文词边界空格保留 | Accepted | `mimotts-core/src/chunking.rs`（含 round-trip 属性测试） |
| ADR-006 | 音频主链路流式 `pcm16`（官方流式要求）→ 24kHz mono PCM16LE 拼接 → 封 WAV；时长按字节精确；非流式 `wav` 作降级 | Accepted | `mimotts-core/src/audio.rs`、`engine.rs::merge_task_audio` |
| ADR-007 | 安全：API Key AES-256-GCM 落盘（`data/master.key`）；API token 仅存 SHA-256 哈希、首启打印一次；默认绑定 127.0.0.1；CORS 本机白名单；SSE 支持 `?token=` | Accepted | `mimotts-core/src/crypto.rs`、`mimotts-api/src/auth.rs`、`serve.rs` |
| ADR-008 | 前端 React 19.2 + TS7 + Vite 8/Rolldown + Tailwind 4 + 虚拟滚动 + SSE 实时；不引入重型组件库 | Accepted | `apps/web/`（DoD 实测：JS 117.5KB/CSS 5.5KB gzip） |
| ADR-009 | 无头优先：同一引擎供 `serve`（WebUI+API）/ `run`（CLI 批量）/ `key` / `migrate`；`--headless` 关闭 UI | Accepted | `crates/mimotts-cli/` |
| ADR-010 | 数据模型 v4：sessions→tasks→chunks 扁平化；状态一律**裸小写字符串**（修 v3 JSON 引号嵌套 bug）；分页 SQL 下推；批量事务 | Accepted | `migrate.rs`、`repo.rs`（`task_status_str`） |
| ADR-011 | 版本统一 v4.0.0（workspace 单一版本源） | Accepted | 根 `Cargo.toml` `[workspace.package]` |
| ADR-012 | 429 智能启停：RPM 90% 头room + TPM 预扣退款；AIMD 并发窗口（冷启动 1、健康 +1/30s、429 减半、上限 16）；全抖动指数退避；3 连 429 熔断 60s 半开单探针；账号级 budget_group 共享配额 | Accepted | `throttle.rs`（AimdGate/BudgetGroup/TokenBucket，12 项单测） |
| ADR-013 | 错误分级：421 内容拦截不重试；401/403/404 配置级错误；400 上下文超限 → 整任务 ×0.8 降档重切（≤2 次）；429/5xx 走 ADR-012 | Accepted | `error.rs`、`mimo.rs::classify`、`engine.rs::rechunk_task` |
| ADR-014 | 设计令牌：小米橙 `#FF6900`（dark `#FF8533`）+ MiSans 字体栈 + 暗色默认双主题（aistudio 风格） | Accepted | `apps/web/src/index.css` `@theme` |

## 后续变更需遵守

1. 改任何 API → 先改 `openapi.yaml` → `npm run gen` → 后端同步。
2. 改分片预算/限流参数 → 只改 `ChunkConfig`/`AimdGateConfig` 常量，带证据更新本表。
3. 状态迁移 → 只允许通过 `task_status_str` 系列裸字符串写入。
