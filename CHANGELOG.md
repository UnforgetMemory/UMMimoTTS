# Changelog

## [Unreleased]

## [4.0.3] - 2026-08-29（umpp 修复与优化）

### Critical（umreview 全量审查实证）
- 前端 `npm run build` 全红（333 个 tsc 错误：测试文件未隔离 + vitest globals 类型缺失）→ tsconfig 补 `vitest/globals`、修复测试类型错误，build 全绿（vitest 371 用例）。
- e2e「错误 scope → 401」原为篡改 token（仅证 HMAC 完整性）→ 改为合法签名 + 异 scope 真绑定断言；provider 编辑用例 restore 原样回写 base_url/budget_group（测试隔离）。
- 契约漂移 CI 不可见 → CI 新增「Copy types into the frontend + git diff 漂移门禁」步骤。

### Must-fix
- chunking force_split 子句边界丢英文词间空格（round-trip 契约破坏）→ 边界后空白并入已 flush 段 + 回归测试；估算器改增量计数（O(L²)→O(L)）；normalize 统一 `char::is_whitespace`（U+3000/NBSP 一致）；rechunk clamp 与 MAX_RECHUNK_DEPTH=3 对齐。
- useTaskStream 任意 taskIds 变化整组断开重连 → per-id 增量协调（仅增删变化通道），退避计数跨重建保留。
- TaskListPage SSE switch 补 `chunk_failed` / `all_chunks_done` 分支。
- endpoints 手写 `TaskListParams` / 会话参数 → 全部改由契约 `paths` 派生（ADR-003）。
- Workbench 试听竞态（慢响应覆盖新音色）→ requestSeq 守卫；VoiceCard 试听按钮不再被 `preview_url` 门控 + 内层按钮键盘事件 stopPropagation；TaskDetailPage 删除后不再 post-action 刷新；ImportPage 会话刷新 seq 守卫。
- mimo SSE：`[DONE]` 后再 poll 返回 Err → done 标志返回 None；400 分类 `token` 子串过宽（"invalid token" 误触发 rechunk）→ 收窄关键词；SSE 解码缓冲 1MiB 上限。
- engine all-failed 路径无原子认领（双 worker 重复 TaskFailed/双扣会话计数）→ `claim_task_failed` 条件 UPDATE；stale-inflight 阈值 120s→600s（大于 5 次重试包络，杜绝双合成）；Assembler 部分写失败回滚截断；shutdown 唤醒 worker；merge 失败事件携带真实错误。
- repo 列表 SELECT 不再拖全文 `content`（`skip_serializing_if` 空串省略）；`updated_at` 补进 SELECT 映射；LIKE 通配符转义 + OFFSET 饱和运算；set_default_provider 单事务；import_legacy_tasks 计数用 `changes()`。
- throttle：health_loop 支持 `close()`（gate 可被回收，替换 provider 时调用）；5xx 不再累计 `consecutive_429`（防非 429 误开熔断）。
- 注释英文化（apps/web 全量 + crates 收尾）；`docs/` gitignore 例外放行 ADR 与两份 v4 plans（README 死链修复）。
- 前端小修：ProviderHealthBar 无数据时不误报「运行正常」+ 仅冷却期计时；backoff 1ms 地板；scoped 短 TTL 半衰期 + generation 防 inflight 回填；Shell 版本号从 package.json 注入；formatDuration 负值回退。

### 测试
- core 33（+force_split 空格回归 / unicode 空白 / rechunk 缩放强化）、engine 27 + assembly 3 + live_mimo 3 + stress 2、vitest 371（+request/authedFetch 错误路径 6、config 失败恢复、theme 真断言、download a.download 断言）、e2e 7（scoped 绑定/隔离修复）。

## [4.0.2] - 2026-08-28（umreview 修复批次）

### Critical（审查实证，全部修复并补测）
- 并发门死锁：`ConcurrencyPermit::drop` 单次 CAS 失败被吞 → `fetch_sub`，新增并发 drop 回归测试。
- 合并双写竞态：`on_chunk_resolved` 无原子认领 → `claim_merge` 条件 UPDATE（仅一个 worker 进入合并）；新增 stale-merging 恢复路径。
- 取消状态机失效：`finish_chunk/fail_chunk` 加状态守卫（inflight 才可 done、done 不可覆写），resolve 遇 cancelled/failed/done/merging 早退。

### Must-fix
- 令牌桶 refill 改 CAS 循环（丢失更新 → 限流超发）。
- rechunk 整任务重切（含 done chunk 作废，seq 重新编号）、**累计缩放（0.8^n，上限 3 次后任务失败）**防活锁。
- submit 空内容前置校验（不落孤儿任务）；优先级语义统一（last_entry + seed/retry 保留优先级）。
- master.key：unix 0o600 直建 + 启动权限复核（过宽即报错），不再存在 0644 窗口。
- 事件总线有界：仅订阅建频道、无订阅不建、FIFO 上限 4096。
- 访问日志脱敏（Logger 格式去掉 query，`?token=` 不再落盘）；`/health` 去重并移入 `/api/v3` 契约路径、载荷最小化。
- import 数量/总量上限（500 文件 / 256MB）；导出 zip 唯一临时名 + 5 分钟延迟清理。
- `--provider` 接线；provider base_url http(s) 校验；zip 文件名长度上限。

### 安全：scoped 签名凭据（方案 B）
- `POST /api/v3/auth/scoped {scope}`：HMAC-SHA256（master key）签发短期（≤900s）、作用域限定（`audio:{id}`/`events:{channel}`/`preview:{voiceId}`）URL 凭据，校验走常量时间 `verify_slice`。
- 原始 API token 不再进 URL；audio/download/preview/events 接受 scoped 或 API token（query）或 Bearer。
- 前端 `getScopedToken` 按 scope 缓存；播放/下载/SSE/试听全部换 scoped。

### 测试
- core 31（+scoped 6）、engine 27（+并发 drop/bus 有界）、压测 1000/1000、e2e 7（+scoped 作用域绑定/错误 scope 401/header 混用拒绝）。

## [4.0.1] - 2026-08-28

### 归档
- v3 旧代码（`backend/` Rust、`frontend/` Vue、`e2e/`、根 `playwright.config.ts`）全量归档至 `.um.agents/archive/local/`（gitignore 保护，git 历史保留原迹）。

### Backend
- **音频接口 `?token=` 鉴权**：`/tasks/{id}/audio`、`/tasks/{id}/download` 支持 query token（原生 `<audio>` 无法带 Header）→ 浏览器可直连做 HTTP Range seek。
- **音色试听白名单代理**：`GET /api/v3/voices/{id}/preview` → 302 到官方 CDN（`aistudio-cdn.xiaomimimo.com` 白名单，SSRF 防护）。
- **Provider 编辑端点**：`PUT /api/v3/providers/{id}`（name/base_url/budget_group）——支持自定义上游与账号预算分组；base_url 变更自动重置该 provider 的 AIMD 运行时。
- 认证收敛：`auth::token_ok` 统一 query/Bearer 双通道（events/audio/download/preview 共用）。
- CLI `run` 输出修复：完成信息正确打印输出目录；`--json` 事件带 `out` 字段。
- 重试策略调优：5xx/网络抖动退避收窄（≤8s，full jitter），MAX_ATTEMPTS 3→5；压测连续两轮 1000/1000（~32s，零失败）。

### 测试
- e2e 新增：token-query 音频 + Range 206、试听代理 302、provider 编辑/恢复；压测失败时输出失败分片诊断。
- 清理了第 1 轮遗留的挂起压测进程（旧二进制队列死锁 bug 的进程残留，与当前代码无关）。

## [4.0.0] - 2026-08-28

> v4 为对 v3（烂尾）的全面重建。调研与规划证据见 `docs/compose/plans/`。

### Backend — Rust workspace（crates/*）

#### Added
- **mimotts-core**：纯领域内核 —— 简化状态机（裸小写序列化）、9 音色/3 模型官方目录（含 `mimo_default`、voiceclone/voicedesign）、8K 上下文智能分片（预算 6000/7500、单一 token 估算器、英文词边界保留、round-trip 属性测试）、24kHz mono PCM16LE WAV 字节数学、AES-256-GCM 密钥密封 + SHA-256 token 哈希。
- **mimotts-engine**：内存优先队列 + `notify` 唤醒 + 空闲退避（修 v3 200QPS 轮询风暴）；AIMD 并发门 + 90% 头room RPM/TPM 双桶 + per-provider 熔断（ADR-012）；官方契约 MimoClient v2（user=风格/assistant=正文、流式 pcm16 SSE 解码、421/429/400 分类）；SQLite schema v4 + SQL 分页下推 + 批量事务 + 每 30s 恢复补种；sessions→tasks→chunks 扁平模型；worker panic 守卫。
- **mimotts-api**：REST v3 + SSE（心跳 + `?token=` 鉴权）+ bearer 中间件 + 结构化错误 + 流式音频（NamedFile/Range）+ 磁盘流式 ZIP 导出 + multipart 批量导入。
- **mimotts-cli**：`serve`（含 `--headless`）/ `run`（无头批量）/ `key issue` / `migrate`；首启 token 引导；127.0.0.1 默认绑定。

#### Fixed（相对 v3）
- 分片默认值 10000/20000 token 超官方 8K 上下文 → 6000/7500 + 自愈降档。
- 状态 JSON 引号嵌套（`"processing"` 带引号）→ 裸小写字符串。
- 假 0.5s 音频时长 → PCM 字节精确计算。
- 全局熔断/重试死码 → per-provider AIMD + 全抖动退避。
- 测试套件编译失败（stress_batch 5/7 元组）→ 全 workspace 测试重建全绿。

### Frontend — React 19.2 重建（apps/web）

- React 19.2 + TypeScript 7.0.2（Go 编译器）+ Vite 8/Rolldown + Tailwind 4（ADR-014 品牌令牌：小米橙/MiSans/暗色默认）。
- 6 路由：工作台/批量导入/任务历史（虚拟滚动+SSE）/任务详情/设置 + Shell。
- 契约先行：openapi-typescript 生成类型接入，零手写业务类型。
- SSE 实时（full-jitter 重连）、Bearer + `?token=` 双通道、providers 频道熔断倒计时。
- 产物预算达标：JS ≈117.5KB gzip（主包 91.6KB）、CSS 5.5KB gzip。

### 验证（2026-08-28 实测）

- `cargo test --workspace` 全绿（core 25 + engine 19 + stress 2）。
- `npm run build` 全绿（tsc 7 + vite 8）；vitest 20 用例全绿。
- 1000 文件压测（wiremock 官方流式契约）全部完成。
- 端到端冒烟：token 引导 → /health → /config（9 音色）→ 建任务 → 失败路径 → 鉴权拦截。

## [3.0.0] 及更早 — 见 git 历史（v3 已冻结于 backend/、frontend/）
