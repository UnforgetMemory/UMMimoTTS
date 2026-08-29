# UM-MimoTTS v4 — MiMo-TTS 自动化流程工作台

![Rust](https://img.shields.io/badge/Rust-1.98-orange.svg)
![React](https://img.shields.io/badge/React-19.2-61dafb.svg)
![TypeScript](https://img.shields.io/badge/TypeScript-7-3178c6.svg)
![Tailwind](https://img.shields.io/badge/Tailwind-4-38bdf8.svg)
![License](https://img.shields.io/badge/license-MIT-blue.svg)

基于 [小米 MiMo-V2.5-TTS](https://mimo.mi.com/models/zh-CN/mimo-v2.5-tts) 的本地自动化流程工作台：
**WebUI 优先 + 无头（headless）双模式**，面向官方 **8K token 上下文**的智能分片、
**429 智能启停**并发控制、大批量 TXT 转换、配置加密与 API 鉴权。

> 本仓库是对 v3（烂尾）的全面重建：v4 为 Rust workspace 内核 + React 19 前端，
> 架构决策见 `docs/compose/plans/2026-08-28-mimotts-workbench-rebuild.md` 与
> `docs/compose/plans/2026-08-28-mimo-adaptation-research.md`。
> v3 旧代码已归档于 `.um.agents/archive/local/`（不入库，git 历史可溯）。

---

## ✨ 特性

- **智能分片**：官方上下文 8K → 分片预算 6000/7500 token（12%+ 余量）；句子/段落边界优先、英文词边界保留、超长句按逗号/停顿硬切；400 上下文超限自动 ×0.8 降档重切（ADR-013）。
- **429 智能启停（ADR-012）**：RPM 90% 头room + TPM 预扣退款双层令牌桶；AIMD 自适应并发窗口（健康 +1/429 减半）；全抖动指数退避；per-provider 熔断（3 连 429 → 60s 半开单探针，渐进恢复）。
- **流式合成**：官方低延迟流式 `pcm16` 逐块解码 → 24kHz mono PCM16LE 拼接 → 封 WAV；时长按字节精确计算（不再有假 0.5s）。
- **大批量 TXT**：多文件拖放导入（UTF-8/GB18030 自动探测）、会话化进度、SSE 实时事件、流式 ZIP 导出。
- **配置安全（ADR-007）**：Provider API Key 以 AES-256-GCM 密文落盘（master key 本地生成）；HTTP API bearer token（首启签发、SHA-256 哈希入库）；默认仅监听 127.0.0.1。
- **官方能力对齐**：9 内置音色（含 `mimo_default`）、3 个 TTS 模型（tts / voicedesign / voiceclone）、风格指令走 `user` 消息、音频标签 `[笑]`/`[吸气]`/`[语速加快]`/`(唱歌)`。
- **极低资源**：SQLite WAL + 每连接 2MB 页缓存（v3 为 64MB×24）、32 worker 默认（流式落盘，O(1) 内存/分片）、空闲队列退避到 500ms（v3 是 50ms×10 轮询风暴）、列表分页全部 SQL 下推。

## 🏗 架构

```
mimotts 单二进制
├─ mimotts-core   纯领域：分片/音频/WAV/加密/事件（无 IO）
├─ mimotts-engine 运行时：调度队列/AIMD 限流熔断/SQLite v4/MiMo 客户端
├─ mimotts-api    REST v3 + SSE + 鉴权 + 静态 UI
└─ mimotts-cli    serve / run / key / migrate（headless 优先）

apps/web          React 19.2 + TS7 + Vite+ + Tailwind 4（设计令牌见小米品牌规范）
packages/contract OpenAPI 3.1 契约（前端类型唯一事实源，openapi-typescript 生成）
```

API 契约（唯一事实源）：`packages/contract/openapi.yaml`。

## 🚀 快速开始

```powershell
# 1. 构建
cd apps/web; npm install; npm run build; cd ../..
cargo build --release -p mimotts-cli

# 2. 启动（默认 http://127.0.0.1:30231）
.\target\release\mimotts.exe serve

# 首次运行会打印 API Token（仅显示一次）→ 在 WebUI 设置页粘贴
# 在设置页配置 Provider API Key（密文落盘），即可开始合成
```

无头批量转换（脱离 UI）：

```powershell
.\target\release\mimotts.exe run -t .\txt -t .\more.txt --voice 冰糖 --style "沉稳" --out .\out --json

# 断点续接：中断/崩溃后不重新导入，直接接管会话直到完成
.\target\release\mimotts.exe run --session <session-id> --out .\out
```

## 🔐 安全模型

| 资产 | 保护方式 |
|---|---|
| Provider API Key | AES-256-GCM 密文落盘；master key 在 `data/master.key`（0600，不落日志） |
| API Token | 仅存 SHA-256 哈希；明文只在签发时打印一次 |
| 网络面 | 默认 `127.0.0.1`；CORS 白名单仅本机来源；`--bind 0.0.0.0` 需显式 |
| 日志 | 全程脱敏，不输出任何 key/token 片段 |

威胁模型：单机本地工具。master key 泄露 = 密钥泄露（文件权限 + 用户目录隔离）。

## 🧪 开发

```powershell
cargo test --workspace            # 单元 + 集成
cargo test -p mimotts-engine --test stress_v4 -- --nocapture   # 1000 文件压测
cargo test -p mimotts-engine --test live_mimo -- --nocapture   # 真实 MiMo API 集成测试（MIMO_API_KEY 放 .env.local，缺省自动跳过）
cd packages/contract && npm run gen   # 契约 → 前端类型
cd apps/web && npm run dev            # 前端热更新（代理 /api/v3 → 30231）
```

本地调优（可选，写入 `.env.local` 或进程环境，无效值忽略）：

```powershell
$env:MIMOTTS_RPM_HEADROOM=95    # RPM 桶容量/速率（默认 90，官方上限 100）
$env:MIMOTTS_TPM_BUDGET=10000000
$env:MIMOTTS_WORKERS=8          # 合成 worker 数（默认 4）
$env:MIMOTTS_CHUNK_TARGET=6000  # 分片目标/硬上限 token（默认 6000/7500，窗口 8K）
$env:MIMOTTS_CHUNK_HARD_CAP=7500
```

性能预算（验收门禁）：空闲 RSS < 50MB · 处理期 < 150MB · 前端 JS ≤ 180KB gzip / CSS ≤ 40KB · 列表接口零全表载入 · 音频下载/导出全程流式。

## 📖 参考

- MiMo-V2.5-TTS 官方文档：https://mimo.mi.com/docs/zh-CN/quick-start/usage-guide/audio/speech-synthesis-v2.5
- 模型与限流：https://mimo.mi.com/docs/zh-CN/quick-start/summary/model
- 设计参考：https://aistudio.xiaomimimo.com

## 📄 License

MIT
