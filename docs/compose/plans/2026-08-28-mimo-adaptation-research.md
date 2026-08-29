# MiMo-V2.5-TTS 深度适配调研附录

> **Created:** 2026-08-28
> **Status:** Research — 为 `2026-08-28-mimotts-workbench-rebuild.md` 的补充调研
> **标记约定:** `Fact`（官方文档/实测）/ `Infer`（由官方文档合理推断）/ `Decision`（据此形成的实现决策）
> **官方来源基线:**
> - API 参考: https://mimo.mi.com/docs/en-US/api/audio/tts （`/v1/chat/completions` OpenAI 兼容）
> - 使用指南: https://mimo.mi.com/docs/zh-CN/quick-start/usage-guide/audio/speech-synthesis-v2.5
> - 模型列表/限流: https://mimo.mi.com/docs/zh-CN/quick-start/summary/model 与 https://mimo.mi.com/docs/zh-CN/api/guidance/rate-limit
> - 错误码: https://mimo.mi.com/docs/zh-CN/api/guidance/error-codes
> - 模型页: https://mimo.mi.com/models/zh-CN/mimo-v2.5-tts

---

## 1. 接口适配手册（逐字段）

### 1.1 认证（`Fact`）

官方支持两种认证方式，二选一加在请求头：
- `api-key: <KEY>`（现有实现所用）
- `Authorization: Bearer <KEY>`

`Decision`：客户端保留 `api-key` 头（兼容现有），同时支持 `Authorization: Bearer`（用户自选/备用渠道）。

### 1.2 请求体（chat completions + audio 模态）

| 字段 | 类型 | 必填 | 约束（`Fact`） |
|---|---|---|---|
| `model` | string | ✅ | `mimo-v2.5-tts` / `mimo-v2.5-tts-voicedesign` / `mimo-v2.5-tts-voiceclone` |
| `messages[].role` | string | ✅ | 顺序固定：`user` 在前、`assistant` 在后 |
| `messages`(user).content | string | 视模型 | 段落级风格/语气/语速指令，**不会被朗读**；`voicedesign` 必填（=音色设计描述） |
| `messages`(assistant).content | string | 视模型 | **待合成正文 + 内嵌标签**；`optimize_text_preview=true` 时可省略 |
| `audio.format` | string | 否 | `wav`（默认）· `mp3` · `pcm` · `pcm16`；**流式时默认 `pcm`**；`pcm` 与 `pcm16` 等价，均指 pcm16 |
| `audio.voice` | string | 视模型 | tts：9 个内置 ID（默认 `mimo_default`）；voiceclone：**必填**，`data:<mime>;base64,...`（仅 mp3/wav，base64 ≤ 10MB）；voicedesign：**不支持此字段** |
| `audio.optimize_text_preview` | bool | 否 | 默认 false；对目标播报文本智能润色，置 true 时可省略 assistant 消息 |
| `stream` | bool | 否 | 低延迟流式已上线（`mimo-v2.5-tts`） |

### 1.3 响应体

**非流式（`Fact`）:** `choices[0].message.audio.data`（base64 音频）· `choices[0].message.audio.transcript`（当前恒为 null）· `usage.completion_tokens` · `usage.prompt_tokens_details.audio_tokens`（音频输入 token）。

**流式（`Fact`）:** SSE 分块 `choices.delta.audio.id` + `choices.delta.audio.data`（base64 音频字节，逐块）；按官方示例 base64 解码 → `np.frombuffer` → 拼接 → `sf.write(..., samplerate=24000)`。

### 1.4 音频规格（`Fact`）

- **24 kHz · mono · PCM16LE**（官方示例注释 `# 24kHz PCM16LE mono audio`）。
- 拼接策略：流式 `pcm16` 逐块解码拼接为纯 PCM，再包 WAV 头（24k/mono/16bit）；非流式直接得到 `wav`。
- `Infer` 时长计算：PCM 字节数 ÷ (24000 × 2) = 秒，可精确到样本。

### 1.5 音色（`Fact`）

| Voice ID | 语言 | 性别 | 备注 |
|---|---|---|---|
| `mimo_default` | 随集群 | — | 中国集群=冰糖，其他集群=Mia（官方说明） |
| `冰糖` / `茉莉` / `苏打` / `白桦` | 中文 | 女/女/男/男 | |
| `Mia` / `Chloe` / `Milo` / `Dean` | English | F/F/M/M | |

现有 `constants.rs` 缺 `mimo_default` → v4 补 9 个并标注集群差异。

### 1.6 风格与标签体系（`Fact`）

三层控制：
1. **段落级指令**（`user` 消息）：自然语言描述语速/情绪/语气，不朗读。
2. **整体风格标签**（assistant 正文**开头**）：`(风格A,风格B)正文`，多个风格同一括号、分隔符不限。官方风格词表：基础情绪（开心/悲伤/愤怒/恐惧/惊讶/兴奋/委屈/平静/冷漠）、复合情绪（怅然/欣慰/无奈/愧疚/释然/嫉妒/厌倦/忐忑/动情）、整体语调（温柔/高冷/活泼/严肃/慵懒/俏皮/深沉/干练/凌厉）、音色定位（磁性/醇厚/清亮/空灵/稚嫩/苍老/甜美/沙哑/醇雅）、人设腔调等。
3. **行内音频标签**（assistant 正文任意位置）：`[吸气]` `[笑]` `[语速加快]` 等，中英双语、开放文本描述、可混用/叠加。
4. **唱歌模式**：正文**最开头** `(唱歌)歌词`，标签等效取值 `唱歌`/`sing`/`singing`；歌词建议中文。

`Decision` UI 提供"风格词快速插入 + 音频标签面板"（点选 `[笑]`/`[吸气]`/`[语速加快]`/`(唱歌)`），并允许自由文本。

### 1.7 错误码（`Fact`，官方 error-codes 页）

| 状态码 | 语义 | 适配动作（`Decision`） |
|---|---|---|
| 400 | 格式/参数/模型/上下文超限等 | 若为上下文超限 → 该片降档重切（×0.8），最多 2 次；其他 → 标记 chunk Failed 并回显信息 |
| 401 | Key 缺失/无效/头格式错误 | Provider 级错误：熔断该 provider，任务级提示"Key 无效" |
| 403 | 无访问权限 | 同上，不重试 |
| 404 | 端点/模型不存在 | 配置错误，不重试 |
| **421** | **内容审核拦截** | **不重试**，chunk Failed + 提示"内容被安全审核拦截"，任务支持"跳过该片重试" |
| **429** | **请求过频，或 Token Plan 额度耗尽** | 见 §2 完整策略 |
| 500/503 | 服务器故障/负载过高 | 指数退避重试（区别于 429 的熔断计数），计入 provider 健康度 |

`Fact` 官方文档**未**声明 `Retry-After` / `x-ratelimit-*` 头 → `Decision` 客户端不依赖响应头，自建预算；实现上机会性读取（若出现则优先尊重）。

### 1.8 限流模型（`Fact`）

- TTS 三模型各自 **RPM 100 / TPM 10M**。
- **RPM/TPM 计算范围 = 同一账号下所有 API Key 对同一模型的请求合计**（跨 Key 聚合，非单 Key）。
- 另有**账号级模型并发上限**（官方未公布具体数值；服务器高负载时可能延迟或 429）。
- 计费：TTS 系列当前限时免费，**不消耗 credits**（Token Plan 场景）；429 也可能由"Token Plan 额度耗尽"触发（免费期内主要来源是过频/并发）。
- 社区实测参考（chat 类模型，`Fact`-第三方）：30 并发全 200（~2.91 req/s）、50 并发出现限流、200 并发全 429；**保守默认并发 ≤20-30**。来源: https://github.com/sleep2agi/agent-network/issues/193

`Decision`：工作台默认并发 4（合成任务，流式长连接为主），可配上限 16；RPM 预算按 **90 头room（≤90/100）**；多 Provider 同账号时合并预算组（见 §2.5）。

### 1.9 现有实现的适配差异清单（`Fact` × 代码）

| # | 现有实现 | 官方要求 | 影响 |
|---|---|---|---|
| 1 | user.content 恒空串 | user 承载风格指令 | 风格/语气控制完全失效 |
| 2 | 8 音色、无 mimo_default | 9 音色 | 缺默认音色 |
| 3 | 仅注册 1 个模型 | 3 个 TTS 模型 | voiceclone/voicedesign 不可用 |
| 4 | `stream:false` 固定 | 流式 + pcm16 为推荐拼接方式 | 长音频延迟高、整包 base64 内存 |
| 5 | 分片 10K/20K token | 上下文 8K | 超长文本必然失败 |
| 6 | `_speed` 死参数 | 无独立 speed 字段（语速走标签/指令） | 语义错位 |
| 7 | 错误仅分 429/5xx/其他 | 421 内容拦截、401/403/404、上下文 400 | 错误处理粗糙 |
| 8 | 限流按单 provider 独立桶 | 账号级聚合 + 并发上限 | 多 Key 同账号会超限 |

---

## 2. 429 智能启停与并发控制（引擎设计规格）

### 2.1 目标（`Decision`）

1. 0 429 不可达，控制 **429 率 < 2%**（5 分钟窗口）；
2. P95 端到端（含排队）< 8s（本工具为离线批量，放宽到任务级可观测即可）；
3. 重试放大 < 1.3×（平均每原始请求尝试次数）；
4. 恢复过程**渐进**：绝不从熔断瞬间跳回满速。

### 2.2 预防层：双层令牌桶 + 预算组

- **RPM 桶**：容量 100，预算使用 ≤90（10% 头room），1 请求/token 计；桶耗尽 → 调度器暂停派发，`notify` 定时唤醒（100ms→空闲 500ms 退避）。
- **TPM 桶**：10M；**派发前预扣**该片估算 token（assistant 正文 + user 风格 + 标签，统一估算器），请求失败/未发出全额退款（现有 `release_tpm` 思路保留）。
- **token 估算与 TPM 的关系**：`Fact` TPM 按"交互 Token"计，含输入与输出；TTS 输出为音频 token（`usage.prompt_tokens_details.audio_tokens`），输入占主导 → 分片预算 6000/7500 已覆盖主体，估算偏差由 10M 量级吸收。
- `Infer` 流式请求 RPM 按请求数计、跨分片时长长 → RPM 压力小，真正的瓶颈是并发上限；因此并发窗口控制（§2.3）比 RPM 更重要。

### 2.3 自适应并发窗口（AIMD）

```
状态: window(当前并发许可), ssthresh, blocked_until
- 健康窗口: 每 30s 且 0 错误 → window = min(window + 1, max_window)   # 加性增
- 收到 429:   ssthresh = max(1, window/2); window = 1;               # 乘性减
             blocked_until = now + backoff(attempt)                  # 全抖动退避
- 收到 5xx:   window = max(1, window - 2); 短退避, 不计入 429 风暴
- 429 连续 ≥3 (每 provider): 熔断 OPEN(60s) → 半开单探针 → 成功才 CLOSED
- 半开探针成功: window = max(1, ssthresh); 之后逐步加性恢复
```
参数（`Decision`）：`max_window` 默认 4、上限 16；`backoff = min(cap=30s, base=1s × 2^attempt)` + **full jitter**（`sleep = rand(0, backoff)`）；attempt 上限 5；421/401/403/404 不计入重试（直接失败）。

### 2.4 与官方行为的对齐

- 尊重 `Retry-After`（若响应出现）；官方未文档化 → 默认全抖动指数退避。
- 429 二义性：官方 429 = "过频 **或** Token Plan 额度耗尽"。引擎无法区分时统一走退避+熔断；连续 429 且退避后仍 429 ≥ 8 次 → 暂停 provider 10 分钟并提示用户"疑似额度问题/检查套餐"。
- 启动快照：进程启动时 window=1，先探针式爬升（避免冷启动满并发打爆账号并发上限）。

### 2.5 账号级预算组（`Fact` 驱动的必要设计）

同一账号多 Key（如 xiaomi + token-plan 三区）共享账号配额：
- provider 配置增加 `budget_group`（默认 `"default"` = 单账号）；
- 同一 budget_group 的 RPM/TPM/并发窗口**合并计算**，任一 key 收到 429 触发组级退避；
- 负载均衡只在**不同账号**之间做（现有 LeastConnections 保留，但按组而非按 key）。

### 2.6 与分片/队列的联动（`Decision`）

- 单片 429 → 重试该片（不重切、不降档）；
- 400 上下文超限 → 该片降档重切（×0.8，最多 2 次）——这是与 429 完全不同的错误通道；
- 熔断期间：已派发 chunk 完成即停，队列整体暂停该 provider 派发；UI 显示"熔断冷却倒计时"（SSE 推送 provider 健康事件）。

---

## 3. 官方样式调研（WebUI Design Tokens）

### 3.1 品牌色（`Fact`）

| Token | Light | Dark | 用途 |
|---|---|---|---|
| 品牌主色 | `#FF6900`（小米橙；token 库亦用 `#FF6700`） | `#FF8533` | CTA / 进度条 / 数字强调 / 选中态 |
| 品牌 hover/渐变暗端 | `#E65100` | — | hover / 次级强调 |
| 强调 pressed | `#FF5C00` | `#FF6700` | 按下态 |
| 品牌软底 | `rgba(255,103,0,.08)` | `rgba(255,133,51,.10)` | active tab / 悬停底 |
| 品牌 ring | `rgba(255,103,0,.20)` | `rgba(255,133,51,.24)` | focus ring |
| 文字主/次/三级 | `#1A1A1A` / `#666666` / `#999999` | 对应反色灰阶 | 正文/描述/注释 |
| 纯白画布 | `#FFFFFF` | — | 浅色底 |

### 3.2 字体（`Fact`）

```
font-sans: MiSans → -apple-system → 苹方(PingFang SC) → 微软雅黑   # 中文/正文
font-mono: Geist Mono → JetBrains Mono → SF Mono                 # ID/时间戳/数值
```
MiSans 免费商用（hyperos.mi.com/font）。`Decision`：Tailwind 4 `@theme` 映射以上字体栈（web 端经 CDN/本地自托管引入 MiSans woff2，离线场景回退系统栈）。

### 3.3 MiMo Studio 界面语言（`Fact` + `Infer`）

- 深/浅**双主题**、Logo 随主题切换（PMKG 站点综述）；
- 首页显著位置展示**最新模型公告**；
- **历史记录**（对话/任务回溯）为一级信息架构；
- 极简：无装饰堆砌，靠层级/留白/品牌色引导。
- `Infer`（视觉共识）：深色主题为默认、橙色仅作功能强调色、卡片化内容区——与主规划 5.9 布局一致。

`Decision` 落地：Tailwind 4 `@theme` 定义 `brand-*`/`brand-soft`/`brand-ring` token；暗色为默认主题 + `prefers-color-scheme` 自动；ID/时间戳/数值一律 `font-mono tabular-nums`；进度条/徽章/CTA 用品牌橙；首页顶部加"模型动态公告条"（从官方 news 页面人工维护或 `/api/v3/config` 下发）。

---

## 4. 对主规划文档的修订（Delta 摘要）

1. **ADR-005 分片**：预算 6000/7500 不变，新增"分片文本 = assistant 正文 + 标签开销计入预算"；风格指令逐片置于 `user` 消息。
2. **ADR-006 音频**：流式 `pcm16` 逐块解码 → 24kHz mono PCM16LE 拼接 → 包 WAV；时长=PCM字节/48000。
3. **新增 ADR-012 429 智能启停**：双层令牌桶（90 头room）+ AIMD 并发窗口 + full jitter 退避 + per-provider 熔断半开 + 账号级预算组。
4. **新增 ADR-013 错误分级**：421 不重试；401/403/404 配置级熔断；400 上下文超限走降档重切；429/5xx 走 §2。
5. **新增 ADR-014 设计令牌**：小米橙 `#FF6900` + MiSans 字体栈 + 暗色默认双主题（§3 token 表入 `apps/web` 的 `@theme`）。
6. **ADR-007 安全**补充：认证头同时支持 `api-key` 与 `Authorization: Bearer`。
7. **模型注册表 v4**：`mimo-v2.5-tts` / `-voicedesign` / `-voiceclone` 三模型 + 9 音色 + 3 音频格式 + 标签面板能力。
8. **并发默认值**：worker 默认 4、上限 16（原规划已定 4，此处补充社区实测依据与 AIMD 上限依据）。

---

## 5. 引用

- 官方 API 参考（EN）：https://mimo.mi.com/docs/en-US/api/audio/tts
- 官方使用指南（ZH，含流式示例/音色表/标签）：https://mimo.mi.com/docs/zh-CN/quick-start/usage-guide/audio/speech-synthesis-v2.5
- 官方模型与限流配额：https://mimo.mi.com/docs/zh-CN/api/guidance/rate-limit
- 官方错误码：https://mimo.mi.com/docs/zh-CN/api/guidance/error-codes
- 模型页：https://mimo.mi.com/models/zh-CN/mimo-v2.5-tts
- OpenAI 限流头与最佳实践：https://developers.openai.com/api/docs/guides/rate-limits
- AIMD/全抖动退避实践：https://www.mfun.ink/en/2026/04/03/claude-api-rate-limit-storm-adaptive-concurrency-backoff-quota-isolation/
- MiMo 并发社区实测：https://github.com/sleep2agi/agent-network/issues/193
- 小米品牌设计语言/设计令牌：https://github.com/XiaoMi/xiaomi-miloco/blob/main/knowledge/07-design/xiaomi-brand-language.md 、`.../design-tokens.md`
- MiSans 字体：https://hyperos.mi.com/font/zh/about/
- MiMo Studio（设计参考）：https://aistudio.xiaomimimo.com/
