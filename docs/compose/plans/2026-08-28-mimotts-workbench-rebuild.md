# UM-MimoTTS v4 重建规划 —— 自动化流程工作台（调研 + 方案）

> **Created:** 2026-08-28
> **Status:** Plan — 待评审（仅调研与规划，未改任何代码）
> **链路位置:** umpp（起点）→ 后续 umcommit / umrelease
> **结论标记:** 全篇使用 `Fact`（证据确凿）/ `Assumption`（合理推断）/ `Decision`（规划决策）

---

## 1. 结论摘要（TL;DR）

1. `Fact` 项目当前 **v3.0.0 处于烂尾状态**：后端 `cargo check` 通过、**`cargo test` 编译失败**（`backend/tests/stress_batch.rs:1718` 元组 5→7 不匹配）；前端 `npm run build` 通过，但前后端字段契约断裂导致 UI 实际不可用（任务标题/进度/时长/风格控制均错位）。
2. `Fact` 官方 API 已变化：MiMo-V2.5-TTS 上下文窗口 **8K tokens**、RPM 100 / TPM 10M、预置 9 音色（含 `mimo_default`）、支持流式（`pcm16`）、风格指令走 `user` 消息、支持音频标签与唱歌模式。**现有默认分片参数（target 10000 / hard cap 20000 token）超出模型上下文，必然失败。**
3. `Decision`（用户已确认）技术方向：**Rust 内核重构 + React 19.2 前端**（"react@9" 不存在，最新稳定为 19.2.x，用户已确认采用 React 19.2）、TypeScript 7（2026-07-08 GA 的原生编译器）、Vite+（VoidZero 统一工具链，beta）、Tailwind CSS 4。
4. `Decision` 目标形态：**WebUI 优先 + 无头（headless）双模式**，单二进制（rust-embed 恢复内嵌前端），支持大批量 TXT 导入转换、面向 8K 上下文的智能分片、配置加密与 API 鉴权。
5. `Decision` 迁移方式：**按波次增量重建**（W0 治理 → W1 Rust 内核 → W2 React 前端 → W3 批量/无头/安全 → W4 压测发布），每波以"构建+测试全绿"为门禁。

---

## 2. 现状诊断（烂尾点证据清单）

### 2.1 项目时间线与版本漂移

| 时间 | 里程碑 | 证据 |
|---|---|---|
| 2026-05-21 | v1.0.0 单文件一体化（README 所述） | `README.md:354` |
| 2026-05-27~31 | v2 批量/分组/看板（大量 Queue 修复） | `CHANGELOG.md:43-143` |
| 2026-06-04 | v3.0.0 分层 DDD + SQLite + SSE 总线 | `CHANGELOG.md:5-40`、`backend/Cargo.toml:3` |
| 2026-06-15 | 最后一次提交（UI v3.2 美化） | `git log -1` → `2026-06-15 17:00:48` |
| **2026-08-28** | 本次调研（烂尾约 2.5 个月） | 会话日期 |

`Fact` 版本漂移：README 宣称 v1.0.0/单文件/前端嵌入，实际后端 v3.0.0、前端 package.json 2.0.0、README 徽章全部过期（`README.md:1-13`）。

### 2.2 构建与测试实测（2026-08-28 本机）

| 验证 | 结果 | 说明 |
|---|---|---|
| `cargo check`（backend） | ✅ 通过 | lib+bin 编译一致 |
| `cargo test`（backend） | ❌ **编译失败** | `tests/stress_batch.rs:1718` 期望 5 元组、`count_by_task_aggregated` 实为 7 元组；说明最近提交未跑 CI 门禁（CI 即 `cargo test`，`ci.yml:47-48`） |
| `npm run build`（frontend） | ✅ 通过 | vue-tsc + vite 8（Rolldown），2468 模块 716ms；但存在运行时契约断裂（见 2.4） |

### 2.3 后端问题（Rust v3.0.0）

**A. 与官方 API 的偏差（高优先级）**
1. `Fact` 分片默认值超模型上下文：`main.rs:97-98` `CHUNK_TARGET_TOKENS=10000`、`CHUNK_HARD_CAP=20000`，而官方 `mimo-v2.5-tts` 上下文窗口 **8K tokens**（官方模型列表）。默认配置下超长文本必然报错。
2. `Fact` 风格控制字段丢失：`infra/mimo/client.rs:96-113` 发送 `user` 消息 content 恒为空串；官方文档明确"`user` 消息用于语气/风格指令"。后端 `style` 字段存在但从未进入请求。
3. `Fact` `speed` 参数为死参数：`client.rs:79-87` `_speed` 下划线未使用；官方以自然语言/音频标签控制语速，API 无独立 speed 字段。
4. `Fact` 双 token 估算器不一致：`infra/mimo/chunker.rs:5-16`（中文×2、ASCII×0.3）与 `:272-286`（中文×1.3、ASCII×0.4）并存，不同路径分片结果不同。
5. `Fact` `tokenize()` 是假异步：`chunker.rs:54-60` 只走本地启发式；注释宣称的"远程 tokenize API"不存在（官方无此端点；测试里 wiremock 的 `/v1/tokenize` 是自造端点，`task_queue.rs:916-924`）。
6. `Fact` 音色表缺 `mimo_default`：`constants.rs:65-130` 只有 8 个音色，官方预置音色为 9 个（含 `mimo_default`，中国集群默认冰糖）。
7. `Fact` 未使用流式：`client.rs:33` `stream: false`；官方已上线低延迟流式（需 `format: "pcm16"`）。批量吞吐下非流式整包 base64 往返延迟更高。
8. `Fact` 未覆盖 3 个 TTS 模型：`constants.rs:146-152` 仅注册 `mimo-v2.5-tts`，官方另有 `mimo-v2.5-tts-voicedesign` / `mimo-v2.5-tts-voiceclone`。

**B. 队列与调度**
9. `Fact` 空闲轮询风暴：`chunk_queue.rs:236-237` 每个 worker 50ms sleep 循环查询 DB；10 并发 ≈ 空闲 200 QPS 打 SQLite，违背"极低资源消耗"。
10. `Fact` 熔断设计矛盾：`chunk_queue.rs:487-491` 3 次连续 429 触发**全局** `paused`，与 `provider_balancer.rs` 声称的 per-provider 熔断（5 失败/60s）双轨并存、互相干扰。
11. `Fact` 重试死码：`chunk_queue.rs:466-470` 两个分支 `(MAX_RETRIES, 30u64)` 完全相同；单 chunk 最多 10 次重试、最长等待 ≈181s。
12. `Fact` 时长硬编码：`chunk_queue.rs:621,662` 缓存命中与成功路径 `mark_done(..., 0.5)` —— chunk 时长元数据恒为 0.5s（仅合并后任务级时长正确）。
13. `Fact` God Object：`task_queue.rs`（1154 行）混装队列编排、合并、batch/group 完成判定、DB 对账；`listen()` 每 10s `find_all()` 全表载入内存 O(N)（`:207-208`），批量场景不可扩展。
14. `Fact` 状态存储嵌套引号：`chunk_repo.rs:105` `serde_json::to_string` 存 `"pending"`（含引号）；`task_queue.rs:316` SQL 直接比对 `'"completed"'` —— 脆弱耦合。
15. `Assumption` 暂停任务忙循环：worker 跳过 `Paused` 任务后未排他标记（`chunk_queue.rs:259-262`），同一 chunk 会被反复取回。
16. `Fact` 批量派发串行 pacing：`batch_service.rs:229-246` 每任务 100ms sleep；1000 文件仅 pacing 即 100s。

**C. 存储与内存**
17. `Fact` 分页在内存做：`routes/tasks.rs:191-230` `find_all()` 后 `skip/take` —— 大批量下 O(N) 内存与查询。
18. `Fact` 大文件整读内存：`routes/tasks.rs:322,473` `std::fs::read`；`batch_service.rs:469` ZIP 全内存构建 —— 长音频（数百 MB）直接内存尖峰。
19. `Fact` SQLite 页缓存 64MB/连接：`db.rs:19` `PRAGMA cache_size=-64000`，池 24 连接（`main.rs:70`）理论峰值 ≈1.5GB；与"极低内存"目标冲突。
20. `Fact` 未内嵌前端：backend 无任何 `rust_embed`/静态服务代码（grep 0 命中），README"单文件一体化"（`README.md:12,89-93`）与 release.yml "embedded frontend"（`release.yml:61-63`）均不成立；生产二进制跑起来没有 UI。
21. `Fact` 无鉴权 + 绑定全网卡：`main.rs:220-224` CORS `allow_any_origin`、`:233` 绑定 `0.0.0.0` —— 局域网任何人可创建/取消/删除任务。

**D. 安全（用户明确要求"配置安全"）**
22. `Fact` API Key 明文落库：`migrate.rs:150` `api_key TEXT NOT NULL DEFAULT ''`、`provider_repo.rs:105-111` 明文写入；`README.md:108` 又宣称"Key 存 localStorage 不上传服务器"——两种说法与实际实现三方矛盾。
23. `Fact` 前端旧逻辑残留：`stores/config.ts:31-40` localStorage `mimo_api_key` 与 provider-repo 双轨，混淆密钥真实存放位置。

**E. 死代码与遗产**
24. `Fact` batch/group 后端 API 全部保留（`routes/batches.rs`、`groups.rs`、`pending_items`/`batch_tasks` 表），但 v3 前端已整体删除批量 UI（commit `72d15c6` "remove batch"）——API 无消费者。
25. `Fact` 状态机虚设：`ChunkStatus::Queued/Dead`、`TaskStatus::Paused/Chunking` 几乎无迁移路径；`#![allow(dead_code)]` 遍布全库（`lib.rs:6` 及各模块）。
26. `Fact` SSE 前端未接线：`useEventSource.ts` 已写但**零调用**（grep 全仓无引用）——实时进度从未生效，任务列表靠手动刷新。

### 2.4 前端问题（Vue 3.5，2.0.0）

27. `Fact` 字段契约断裂（烂尾核心症状）：`types/task.ts:3-22` 使用 `custom_title/text/has_audio/token_count/current_chunk/error`，而后端返回 `title/content/total_tokens/output_path/...`（`routes/tasks.rs:76-102`）→ `TaskList.vue:44`、`TaskDetail.vue:28-46` 永远走兜底（"任务 xxxxxxxx"、进度 0、无音频入口）。
28. `Fact` `CreateTaskRequest` 发 `context` 字段（`types/task.ts:42-48`、`SynthesizeForm.vue:247`），后端结构体无此字段（`routes/tasks.rs:10-21` 为 `style`）→ 风格控制被静默丢弃。
29. `Fact` 列表筛选静默失效：`api/tasks.ts:6-12` 传 `status/search`，后端 `ListTasksQuery` 无这两项（`routes/tasks.rs:24-31`）。
30. `Fact` 迁移残留：双 `App.vue`（`frontend/App.vue` 与 `frontend/src/App.vue` 两套设计并存）、双 `style.css`、双 playwright 配置（根目录 + `frontend/`）、双 e2e 目录（`e2e/` + `frontend/e2e/`）。
31. `Fact` `lucide-vue-next@1.0.0` 已废弃（npm 安装警告）→ 迁 `@lucide/vue`（本次将整体换 React 栈，顺带解决）。
32. `Fact` 构建产物：index chunk 192KB(73KB gz) + button chunk 37KB + CSS 91.6KB（`npm run build` 实测）——shadcn 全家桶引入，虚拟滚动未启用（changelog 声称的 `@tanstack/vue-virtual` 已随 v3 重写移除）。

### 2.5 仓库卫生

33. `Fact` 二进制入库：`backend/task_texts.db`、`backend/text_files/index.db`、`stress_results/`、`.omo/` 均在 git 跟踪中（`git ls-files` 实测）。
34. `Fact` `.gitignore:46` 忽略 `docs/`，但 `docs/compose/plans/...` 又已入库（先入库后加 ignore 的典型漂移）。
35. `Fact` CI 无前端 e2e、无 lint/format 门禁（`ci.yml` 仅 build+test）；`Cargo.lock` 被 gitignore（`backend` 依赖无法复现构建）。

---

## 3. 外部调研

### 3.1 MiMo-V2.5-TTS 官方 API（`Fact`，来源：官方文档）

**模型与限制**（[官方模型列表](https://mimo.mi.com/docs/zh-CN/quick-start/summary/model) / [语音合成使用指南](https://mimo.mi.com/docs/zh-CN/quick-start/usage-guide/audio/speech-synthesis-v2.5)）：

| 项目 | 值 |
|---|---|
| 模型 ID | `mimo-v2.5-tts`（预置音色+唱歌）· `mimo-v2.5-tts-voicedesign`（文本设计音色）· `mimo-v2.5-tts-voiceclone`（音频克隆音色） |
| 上下文窗口 | **8K tokens**；最大输出 8K |
| 限流 | RPM 100 / TPM 10M（应用级） |
| 计费 | 当前限时免费 |
| 端点 | `POST {base}/v1/chat/completions`（OpenAI 兼容；`api-key` header） |
| 消息约定 | **待合成文本放 `assistant` 消息**；`user` 消息为可选风格/语气/语速指令（不出现在语音中）；`voicedesign` 时 `user` 必填 |
| 音频格式 | `audio.format`: `wav` / `mp3` / `pcm16`；**流式时必须 `pcm16`**（逐块 base64 拼接）；另有 `optimize_text_preview` 参数 |
| 音色 | `audio.voice`: 9 个内置 —— `mimo_default`（中国集群默认冰糖，其他集群默认 Mia）、`冰糖`、`茉莉`、`苏打`、`白桦`、`Mia`、`Chloe`、`Milo`、`Dean` |
| 控制能力 | 自然语言风格指令（语速/情绪/语气）+ 音频标签（如 `[吸气]` `[笑]` `[语速加快]`）+ `(唱歌)` 唱歌模式 |
| 流式 | `mimo-v2.5-tts` 低延迟流式已上线，实时返回 |

**官方调用示意（非流式）：**
```json
POST /v1/chat/completions
{ "model": "mimo-v2.5-tts",
  "messages": [
    { "role": "user",      "content": "用温柔的语气，语速稍慢" },
    { "role": "assistant",  "content": "要合成的正文文本……" }
  ],
  "audio": { "format": "wav", "voice": "冰糖" } }
```
（来源：官方 usage-guide 页面 Python 示例；`mimo.mi.com/models/zh-CN/mimo-v2.5-tts` 模型页同源。）

**对现有实现的直接影响（已在 2.3.A 列出）**：分片预算必须以 8K 为硬上限；风格写入 `user` 消息；流式走 `pcm16`；补齐 `mimo_default` 音色与 3 模型注册表。

> 深度适配细节（逐字段请求/响应、错误码语义、429 智能启停算法、官方样式令牌）见专项附录：`docs/compose/plans/2026-08-28-mimo-adaptation-research.md`

### 3.2 技术栈可用性（`Fact`，来源：官方发布渠道）

| 技术 | 现状（2026-08） | 结论 |
|---|---|---|
| **TypeScript 7** | [2026-07-08 GA](https://devblogs.microsoft.com/typescript/announcing-typescript-7-0/)：Go 原生编译器，全量构建 8–12× 加速；`tsc` 命令不变；部分旧配置默认值升级为硬错误 | 采用；需在 CI 验证 viteplus/编辑器兼容 |
| **Vite+** | [2026-07 发布 beta](https://voidzero.dev/posts/announcing-vite-plus-beta)：`vp` CLI 统一 Vite 8/Rolldown/Vitest/tsdown/Oxlint/Oxfmt/Vite Task；支持 `vp migrate` | 采用（`viteplus.dev`） |
| **Tailwind CSS 4** | 4.x 现行（现有前端已用 ^4.3.0 + `@tailwindcss/vite` 插件） | 沿用 |
| **React** | [最新稳定 19.2.x](https://react.dev/versions)（19.2.8, 2026-07-21）；**不存在 React 9** | `Decision`（用户确认）采用 **React 19.2** |
| 本机工具链 | Node 24.19 / npm 11.17 / Rust 1.98 | 满足构建要求 |

### 3.3 设计参考（aistudio.xiaomimimo.com）

`Fact` 该站点为 **Xiaomi MiMo Studio**（官方网页版对话/多模态体验台）。可借鉴的设计语言：左侧导航 + 主工作区两栏结构、深色优先 + 明亮切换、顶部品牌条、任务/会话式信息密度、卡片化与克制配色。规划中作为 WebUI 布局蓝本（见 5.9），不照搬其聊天形态——本产品是**流程工作台**而非对话台。

---

## 4. 问题定义（P1）

### 4.1 Problem Statement

**What:** 将烂尾的 UM-MimoTTS（v3，Rust + Vue）重建为 **MiMo-TTS 自动化流程工作台**：一个 WebUI 优先、可无头运行的本地服务，支持安全配置、大批量 TXT 导入转换、面向 MiMo 8K 上下文的自适应智能分片、实时进度监控与音频产出管理；后端 Rust 内核重构，前端 React 19.2 + TS7 + Vite+ + Tailwind 4 重建。

**Why:** 现有代码已无法可靠使用——测试编译不过、前后端契约断裂（任务列表/详情/风格控制均失效）、分片参数超过官方上下文必然失败、无鉴权无加密、批量能力只存在于无 UI 的后端、单二进制分发承诺已失效。

**Scope（重建范围）:**
- 后端 Rust workspace：领域模型精简、分片引擎重写（对齐 8K）、队列/限流/熔断重构、SQLite schema v4 + 迁移工具、REST v3 + SSE、内嵌前端静态资源、CLI 无头模式、安全（AES-GCM 密钥落盘 + API 鉴权）。
- 前端：React 19.2 SPA 全量重建（工作台/批量导入/任务与历史/设置/任务详情），OpenAPI 生成类型杜绝契约漂移。
- 工具链与 CI：TS7 + Vite+（vp）、Tailwind 4、Playwright e2e、修复测试编译、发布流水线恢复"单二进制含 UI"。
- 文档：README/CHANGELOG/ADR 重建。

**Non-goal（本期不做）:**
- 不做多用户账户体系（单机本地工具定位；只做本地 API token 鉴权）。
- 不做云端部署/多实例分布式队列（SQLite 单机即可）。
- 不做音色克隆/设计的完整 UI（后端模型注册表预留扩展点，UI 列为 v4.1 候选）。
- 不做 TTS 之外的模型（文本/视觉）接入。

### 4.2 成功标准（可验证）

1. `cargo test` 与 `vp check` 全绿，CI 全绿。
2. 单二进制 `mimotts` 内嵌 UI：`serve` 启动 < 100ms、空闲常驻内存 < 50MB（见 5.11 预算）。
3. 批量导入 1000 个 txt 无 OOM、队列按 100 RPM 上限稳定饱和、进度经 SSE 实时可见。
4. 超长文本（>8K token）自动分片且逐片可合成、结果无缝合并。
5. API Key 落盘密文；HTTP API 需 token；默认绑定 127.0.0.1。
6. 无头模式：`mimotts run` 可脱离 UI 完成相同任务。

---

## 5. 工程规格（P2）

### 5.1 目标形态

```
┌──────────────────────────────────────────────────────────────┐
│  mimotts 单二进制 (Rust)                                     │
│  ┌──────────────────┐   ┌──────────────┐   ┌──────────────┐ │
│  │ HTTP+SSE (actix) │←──│ Engine 内核  │──→│ MIMO 云 API   │ │
│  │ /api/v3/*  + UI  │   │ 分片/队列/    │   │ 8K ctx/100RPM │ │
│  └──────────────────┘   │ 限流/合并/安全│   └──────────────┘ │
│   WebUI(内嵌 React19)   └──────────────┘                     │
│   CLI 无头: mimotts run / serve --headless / key             │
└──────────────────────────────────────────────────────────────┘
```

### 5.2 架构总览（monorepo v4）

```
um-mimotts/
├─ crates/                        # Rust workspace
│  ├─ mimotts-core/               # 纯领域：模型/事件/分片/合并/加解密（无 IO）
│  ├─ mimotts-engine/             # 运行时：调度器/限流/熔断/恢复/缓存（tokio）
│  ├─ mimotts-api/                # actix-web：REST v3 + SSE + 静态内嵌 + 鉴权
│  └─ mimotts-cli/                # clap CLI：serve / run / key / migrate
├─ apps/web/                      # React 19.2 + TS7 + Tailwind 4
│  └─ src/{api-gen,routes,features,components}
├─ packages/contract/             # OpenAPI 3.1 契约（唯一事实源）+ 生成脚本
├─ e2e/                           # Playwright（统一目录，删除双份）
├─ docs/{adr,compose/plans}       # ADR 与规划
└─ .github/workflows/             # ci + release（修嵌入打包）
```

### 5.3 ADR 决策记录（本次规划内定，执行期逐条落 `docs/adr/`）

| # | 决策 | 理由 |
|---|---|---|
| ADR-001 | 后端保留 Rust（tokio + actix-web 4），不做 TS 服务端 | 极致性能/低内存/无头 daemon 定位；`Decision` 用户确认 |
| ADR-002 | 不迁移 axum，沿用 actix-web | 现有依赖已验证；重构焦点在领域与队列，框架迁移零收益 |
| ADR-003 | 契约先行：OpenAPI 3.1 为单一事实源，`openapi-typescript` 生成前端类型；Rust serde 结构同步维护 | 根治 2.4.27-29 类契约断裂 |
| ADR-004 | 队列=内存优先队列 + SQLite 持久恢复，`notify` 唤醒取代轮询 | 消除 2.3.B9 轮询风暴；保留崩溃恢复能力 |
| ADR-005 | 分片预算：目标 6000 token、硬上限 7500（官方 8K，留 ≥12% 余量）；单一校准估算器；风格指令逐片携带；超限自动降档重切 | 2.3.A1/A4/A5 修复 + 失败自愈 |
| ADR-006 | 音频主链路改流式 `pcm16`：逐块解码写入 → 拼 PCM → 封 WAV（时长按 PCM 字节精确计算）；非流式 `wav` 作降级 | 官方推荐拼接方式；消除 2.3.B12 假时长 |
| ADR-007 | 安全：API Key AES-256-GCM 落盘（master key 文件，Windows 可选 DPAPI）；HTTP API bearer token（首启生成、哈希存库）；默认绑定 127.0.0.1；CORS 白名单本机 | 2.3.D 全部项 |
| ADR-008 | 前端 React 19.2 + TS7 + Vite+ + Tailwind 4；shadcn/ui(react) 按需引入 + @tanstack/react-virtual + SSE 实时状态；不引入重型组件库 | 启动/渲染性能预算 |
| ADR-009 | 分发：rust-embed 内嵌 `apps/web/dist` 恢复单二进制；`--ui/--headless` 开关；dev 模式 Vite 代理 | 修复 2.3.C20 |
| ADR-010 | 数据模型 v4：删除 batches/groups/pending_items/batch_tasks 四表与对应 API，改为 `sessions`（导入会话）+ 扁平 `tasks/chunks`；状态机精简 | 2.3.E24-25 清理 |
| ADR-011 | 版本统一 v4.0.0（Cargo/前端/README 同源注入） | 2.1 漂移修复 |
| ADR-012 | 429 智能启停：双层令牌桶（RPM 90% 头room + TPM 预扣退款）+ AIMD 并发窗口 + full jitter 退避 + per-provider 熔断半开 + 账号级预算组 | 附录 §2；官方限流为账号级跨 Key 聚合 |
| ADR-013 | 错误分级：421 内容拦截不重试；401/403/404 配置级熔断；400 上下文超限降档重切（×0.8，≤2 次）；429/5xx 走 ADR-012 | 附录 §1.7 |
| ADR-014 | 设计令牌：小米橙 `#FF6900`（dark `#FF8533`）+ MiSans 字体栈 + 暗色默认双主题，Tailwind 4 `@theme` 落地 | 附录 §3 |

### 5.4 模块规格

**mimotts-core（纯函数/类型，无 IO）**
- `domain`：`Task/Chunk/Session/Provider` + 精简状态机（Task: `Pending→Queued→Synthesizing→Merging→Done|Failed|Cancelled`；Chunk: `Pending→InFlight→Done|Failed`）
- `chunking`：分片引擎（见 5.6）
- `audio`：WAV 头读写、PCM 拼接、时长计算（迁移并强化现有 `merger.rs`）
- `crypto`：AES-256-GCM 封装（测试密钥轮换接口）

**mimotts-engine（tokio 运行时）**
- `scheduler`：内存优先队列（BTreeMap<priority, VecDeque>）+ `notify`；启动时从 DB 回填；chunk 认领改原子 UPDATE
- `throttle`：per-provider token bucket（RPM/TPM 双桶，保留 `rate_limiter.rs` 的 CAS 实现，删除重复分支与全局 paused）
- `circuit`：per-provider 熔断（阈值 5 / 60s / 半开探测，迁移 `provider_balancer.rs`）
- `recovery`：合并 watchdog/patrol/chunk_recovery 为单一循环（SQL 条件查询，禁止 `find_all()` 全表）
- `storage`：SQLite（schema v4）+ 分页下推 SQL + 批量事务
- `bus`：事件总线（task/session 两级）+ SSE fan-out（迁移 `sse_bus.rs`）

**mimotts-api**
- REST v3：`/api/v3/{sessions,tasks,chunks,providers,config,events,audio}`；音频下载/导出走流式响应（NamedFile/Range）
- 鉴权中间件（bearer token）+ 限速（本机 API 防滥用）+ 结构化错误
- 静态资源：rust-embed + 内容缓存头

**mimotts-cli**
- `mimotts serve [--ui|--headless] [--port]`
- `mimotts run --txt <files|dir> --voice --model --style --out [--concurrency] [--json]`
- `mimotts key {show,rotate,set}`、`mimotts migrate`

### 5.5 数据模型（SQLite schema v4）

```sql
providers(id, name, base_url, kind, api_key_enc BLOB, is_default, ...)   -- 密文存储
sessions(id, name, status, total_tasks, done_tasks, failed_tasks, created_at, ...)
tasks(id, session_id NULL, title, content, voice, model,
      style,                    -- 写入 user 消息
      tags,                     -- 音频标签/唱歌模式
      status, priority, total_chunks, done_chunks, failed_chunks,
      output_path, duration_ms, provider_id, error,
      created_at, updated_at, completed_at)
chunks(id, task_id, seq, text, token_estimate, status, retry_count,
       audio_path, duration_ms, error, created_at, updated_at)
config(key, value)              -- 服务端设置（限流、绑定地址等，不含密钥）
api_tokens(id, token_hash, label, created_at)
-- 索引：tasks(status, created_at)、chunks(status, priority, created_at) 等
```

关键点：状态一律存**纯小写裸字符串**（`"pending"`，不带 JSON 引号）；分页/统计全部 SQL 下推（`LIMIT/OFFSET + COUNT(*)`，或 keyset 游标）；批量为单事务插入。

### 5.6 智能分片引擎（核心需求）

规格（对齐官方 8K 上下文）：
1. **预算**：目标 6000 token / 硬上限 7500 token（`Decision` 默认，可在 config 调低）。
2. **管道**：文本规范化（全角标点/换行/空白）→ 段落切分 → 句子切分（`。！？…!?;；\n` + 右引号闭合）→ **单一 token 估算器**（CJK 与拉丁分开计权，常数随实测校准；留 20% 安全余量）→ 句子级贪心装箱 → 超长句二次切分（优先在逗号/顿号/连词处落刀）。
3. **风格携带**：每片请求独立构建 `user` 消息（style + 可选续写提示），避免跨请求无状态导致风格漂移。
4. **自愈**：若服务端返回上下文超限类错误（400/上下文错误码），自动以 0.8 系数缩小预算重切该片，最多 2 次。
5. **可观测**：每片记录 `token_estimate`，任务级聚合展示分片数/预算占用率。
6. **可测**：纯函数 + 属性测试（任意文本切分后按 seq 拼接 == 原文，除分隔符空白外无损）。

### 5.7 队列与并发

- 默认 worker 数 4（可配），task 级与 chunk 级双层并发上限；调度器以"会话内 FIFO + 全局优先级"排序。
- per-provider 双桶限流：RPM 使用 90% 头room（默认 90/100）、TPM 10M；爆桶时 worker 挂起于 `notify` + 定时唤醒（≤100ms，空闲自动退避至 500ms）。
- 重试：429/5xx 3 次指数退避+jitter；网络错误独立计数；per-provider 熔断（阈值 5、恢复 60s、半开）。
- 崩溃恢复：启动扫描 + 周期扫描 InFlight 超时 chunk 复位；不再需要 watchdog/patrol/recovery 三套循环并存。
- 批量导入：`sessions` 事务内批量插 task；chunk 生成与 API 请求异步解耦。

### 5.8 配置与安全

- `config.toml`（服务端配置：端口/绑定/限流/缓存/路径）+ 环境变量覆盖（`MIMOTTS_*`）。
- 密钥体系：master key 文件（`data/master.key`，随机生成、OS 权限收紧；Windows 后续可加 DPAPI）→ 加密 provider API key 与（可选）token 相关材料；日志脱敏（任何错误消息不回显 key 片段）。
- HTTP API：首启生成 bearer token 打印一次并哈希入库；`Authorization: Bearer` 中间件；WebUI 登录页/设置页换 token。
- 网络面：默认 `127.0.0.1`，`--bind 0.0.0.0` 需显式 + 日志警告；CORS 仅允许本机来源。

### 5.9 WebUI（React 19.2，参考 aistudio 布局）

- **技术**：React 19.2 + `react-router`（v7）、TanStack Query（服务端状态）+ zustand（UI 状态）、SSE hook（自动重连/Last-Event-ID）、`@tanstack/react-virtual`、shadcn/ui（React 版，按需引入）、Tailwind 4、TS7 严格模式。
- **布局**（借鉴 MiMo Studio 信息架构）：左侧窄导航（工作台 / 批量导入 / 任务历史 / 设置）→ 主工作区；顶栏品牌 + 运行状态（队列深度/限流余量/熔断指示灯）。
- **页面**：
  1. **合成工作台**：文本输入 + 风格指令 + 9 音色卡片（CDN 试听）+ 模型选择 + 音频标签（`[笑]` `[语速加快]` `(唱歌)`）+ 提交后进度条（SSE）。
  2. **批量导入**：多 txt 拖放/选择、编码探测（UTF-8/GBK）、每文件标题/音色覆盖、会话级进度、完成 ZIP 导出。
  3. **任务历史**：虚拟滚动列表（状态徽章/进度/时长）+ 筛选 + 分页；行点击进详情。
  4. **任务详情**：音频播放器（波形）+ 分片列表（每片状态/重试）+ 文本预览 + 重试/取消。
  5. **设置**：Provider 密钥（密文保存、覆盖式输入）、默认音色/模型、限流参数、API token、数据目录、无头模式说明。
- **性能措施**：路由级 code-split；虚拟滚动；SSE 增量更新（不整页刷新）；首屏关键 CSS 内联；`vp build` + Rolldown 产物预算（见 5.11）。

### 5.10 无头模式（headless）

- 同一引擎/同一契约，CLI 是 API 的另一个客户端：`mimotts run` 内置最小客户端逻辑（不依赖浏览器）。
- `--json` 输出机器可读进度（任务/分片事件流），便于嵌入自动化流水线。
- `serve --headless`：仅 API 端口，不挂 UI 路由（少一个攻击面，内存更小）。

### 5.11 性能预算（`Decision`，压测门禁）

| 指标 | 目标 | 验证方式 |
|---|---|---|
| 空闲常驻内存 | < 50 MB | `mimotts serve` 稳态 RSS 采样 |
| 处理期内存（4 并发） | < 150 MB | 1000 文件批量 + 峰值 RSS |
| SQLite 页缓存 | 全池 ≤ 32 MB（`cache_size=-8000`×池 4） | PRAGMA 审计 |
| 冷启动 → 可服务 | < 100 ms | 时间戳实测 |
| 前端首屏（localhost） | 交互 < 1s；JS ≤ 180KB gz / CSS ≤ 40KB | `vp build` 产物审计 + Lighthouse |
| 队列利用率 | ≥ 90% RPM 持续饱和度（不超官方 100 RPM） | 批量实测统计 |
| 流式首块延迟 | P50 < 2s（网络外因素） | 指标埋点 |
| 大数据接口 | 任何列表接口禁全表载入；音频下载/导出流式 | 代码评审 + 压测 |

### 5.12 测试与 CI

- **Rust**：unit（chunking 属性测试、crypto、merger、token bucket）+ integration（wiremock 按**官方契约**模拟：8K 超限错误、429、流式 pcm16 分块；删除自造 `/v1/tokenize` mock）。
- **前端**：Vitest（关键 hook/组件）+ Playwright e2e（合成全链路 / 批量导入 / 无头 CLI 冒烟 / 并发用户），统一到 `e2e/`。
- **CI**：`vp check`（TS7 + oxlint）→ `cargo test` → 前端 build → release 单二进制打包（内嵌前端验证 UI 可达）；修复 `Cargo.lock` 入库。
- **门禁顺序**：语言工具链 → 构建 exit 0 → 测试全绿 → 文档同步 → 证据齐全。

---

## 6. 迁移策略与波次

采用**替换式增量**（strangler）：新内核/新前端在并行目录开发，旧目录冻结；契约 v3 与 v4 不追求兼容（单机工具、无外部消费者），一次性切换 + `mimotts migrate` 从旧库导入历史任务（若用户需要）。

| 波次 | 内容 | 验收 |
|---|---|---|
| **W0 治理** | 仓库卫生、monorepo 骨架、契约工具链、CI 修复、版本统一 | CI 全绿（含修复后的 cargo test） |
| **W1 Rust 内核** | core/engine/api v3 + schema v4 + 分片/流式/安全 + CLI 骨架 | 后端集成测试全绿；无头 `run` 可用 |
| **W2 React 前端** | 五页面 + SSE + 虚拟滚动 + 生成类型接入 | e2e 全绿；性能预算达标 |
| **W3 批量+安全收口** | 多 txt 导入/编码探测/ZIP 导出、密钥加密与 token、单二进制内嵌 | 1000 文件压测 + 安全项人工审计 |
| **W4 发布** | 性能压测调优、README/CHANGELOG/ADR、release 流水线 | 四平台产物 + UI 随包验证 |

---

## 7. 原子化 TODO（P3）

> 每项独立可验证；依赖以 `← T<x.y>` 标注；执行期按波次推进，禁跨波跳跃。

**W0 治理**
- T0.1 删除/解除跟踪二进制与运行产物（`backend/*.db`、`stress_results/`、`.omo/`）并补 `.gitignore` 规则
- T0.2 统一删除前端双份残留（根 `frontend/App.vue`、`style.css`、`playwright.config.ts`、`frontend/e2e/`）
- T0.3 建立 workspace：`crates/*`、`apps/web`、`packages/contract`、`e2e/` 目录骨架；`Cargo.lock` 入库
- T0.4 修复 `backend/tests/stress_batch.rs:1718` 编译错误（7 元组适配）使 `cargo test` 恢复绿色
- T0.5 引入 TS7 + Vite+（`vp migrate`）+ Tailwind 4 + React 19.2 的空壳 `apps/web`，CI 加 `vp check`
- T0.6 OpenAPI 3.1 契约初稿（`/api/v3` 任务/会话/分片/事件）+ 生成脚本打通

**W1 Rust 内核**
- T1.1 `mimotts-core` 领域类型与 v4 状态机 + 单元测试 ← T0.3
- T1.2 schema v4 迁移器 + `mimotts migrate`（旧库 tasks 导入，状态字符串规范化去引号）← T1.1
- T1.3 分片引擎重写（预算 6000/7500、单一估算器、段落/句子/硬切、自愈降档）+ 属性测试 ← T1.1
- T1.4 MimoClient v2（官方契约：user=风格、assistant=正文、9 音色、3 模型、流式 pcm16 解码落盘）+ wiremock 集成测试 ← T1.3
- T1.5 调度器重构（内存队列+notify、原子认领、空闲退避）+ 负载/恢复单循环 ← T1.2
- T1.6 限流/熔断收敛（per-provider 双桶、删全局 paused 与重试死码、90% 头room）← T1.5
- T1.7 合并与时长（PCM 拼接封 WAV、按字节精确时长）← T1.4
- T1.8 存储层（分页下推、批量事务、PRAGMA 调优：cache_size/池/单写者）← T1.2
- T1.9 安全内核（AES-256-GCM + master key + token 哈希 + 日志脱敏）← T0.3
- T1.10 REST v3 + SSE + 鉴权中间件 + 结构化错误（对齐 T0.6 契约）← T1.2,T1.5,T1.9
- T1.11 CLI：`serve --headless` / `run` / `key`（clap + 最小客户端）← T1.10
- T1.12 后端集成测试套件全绿 + `cargo test` 门禁 ← T1.3..T1.11

**W2 React 前端**
- T2.1 `apps/web` 布局骨架（左侧导航+顶栏+主题切换，aistudio 风格基调）← T0.5
- T2.2 生成类型接入 + API 客户端 + SSE hook（自动重连）← T0.6,T1.10
- T2.3 合成工作台（文本/风格/音色卡+试听/音频标签/模型）← T2.2
- T2.4 任务历史（虚拟滚动 + 筛选 + 分页 + SSE 实时状态）← T2.2
- T2.5 任务详情（播放器 + 分片进度 + 重试/取消）← T2.2
- T2.6 设置页（Provider 密钥/默认值/限流/token）← T1.9,T1.10
- T2.7 批量导入页（多文件拖放/编码探测/覆盖项/会话进度/ZIP 导出）← T1.10
- T2.8 前端单测 + Playwright e2e（全链路/并发/无头冒烟）← T2.3..T2.7
- T2.9 性能预算验收（分片/virtual/产物体积）← T2.3..T2.7

**W3 批量+安全收口**
- T3.1 后端批量导入端点（multipart 多文件 + 单事务 + 会话模型）← T1.8,T1.10
- T3.2 ZIP 导出流式化（不整包内存）← T1.8
- T3.3 音频下载流式化（NamedFile/Range）← T1.10
- T3.4 rust-embed 内嵌前端 + `--ui` 开关 ← T2.9
- T3.5 1000 文件压测（内存/队列利用率/时长）+ 预算达标 ← T3.1
- T3.6 安全人工审计清单（密文落盘/脱敏/默认绑定/权限）← T1.9

**W4 发布**
- T4.1 README/CHANGELOG/ADR 全套重建（v4.0.0）← 全波
- T4.2 release.yml 修复（内嵌 UI 验证、`Cargo.lock`）← T3.4
- T4.3 多平台产物冒烟（Windows/macOS/Linux）← T4.2
- T4.4 umcommit → umrelease 发布链路走查

---

## 8. 风险与缓解

| 风险 | 等级 | 缓解 |
|---|---|---|
| 官方"限时免费"结束/限流收紧 | 中 | provider 可插拔 + 限流全量配置化；收费后仅改成本提示 |
| 8K 上下文无官方 tokenize 端点，估算器偏差 | 中 | 20% 安全余量 + 超限自动降档重切（5.6.4）+ 实测校准常数 |
| 流式 pcm16 中途断流 | 中 | 分片级重试幂等（chunk 状态机 + 原子认领）；非流式 wav 降级路径 |
| React 19.2 + TS7 + Vite+（beta）组合兼容 | 中 | W0 先立工具链空壳跑 CI；版本锁定；失败则退 Vite 8 官方插件 |
| 旧库数据迁移（状态带引号/旧表） | 低 | `mimotts migrate` 独立命令，默认仅导入 tasks；旧表不删只读 |
| 单机 SQLite 写并发 | 低 | 单写者 + WAL + busy_timeout；批量单事务 |
| 重建范围大、周期长 | 中 | 波次化 + 每波独立验收；旧代码冻结不双写 |

---

## 9. 开放问题（不阻塞开工，执行期逐项确认）

1. 是否需要保留旧库历史任务数据迁移（默认：提供工具，不强制）。
2. `mimo-v2.5-tts-voicedesign / voiceclone` 是否纳入 v4.0 UI（默认：后端注册表预留，UI v4.1）。
3. 输出格式是否需要 mp3（官方支持，成本低；默认 wav + 可选 mp3）。
4. 品牌命名沿用 "UM-MimoTTS v4" 是否 OK。

---

### 附：本规划全部外部证据 URL

- 官方模型列表（8K/RPM/TPM）：https://mimo.mi.com/docs/zh-CN/quick-start/summary/model
- 语音合成使用指南：https://mimo.mi.com/docs/zh-CN/quick-start/usage-guide/audio/speech-synthesis-v2.5
- 模型页：https://mimo.mi.com/models/zh-CN/mimo-v2.5-tts
- MiMo Studio（设计参考）：https://aistudio.xiaomimimo.com
- TypeScript 7 GA：https://devblogs.microsoft.com/typescript/announcing-typescript-7-0/
- Vite+ beta：https://voidzero.dev/posts/announcing-vite-plus-beta · https://viteplus.dev
- React 版本：https://react.dev/versions（最新 19.2）
- React releases：https://github.com/react/react/releases
