# apps/web — UM-MimoTTS v4 工作台前端

React 19.2 + TypeScript 7 + Tailwind CSS 4 + Vite 8（Rolldown）。参考 Xiaomi MiMo Studio
（aistudio.xiaomimimo.com）信息架构：左侧窄导航 + 顶栏 + 主工作区，深色默认、橙色仅作功能强调。

## 命令

```bash
npm install
npm run dev          # 启动 Vite，代理 /api/v3 → http://127.0.0.1:30231/api/v3
npm run build        # tsc 类型检查 + vite build
npm test             # vitest 单元测试
npm run typecheck    # 仅类型检查
```

## 类型生成（ADR-003 契约先行，禁止手写业务类型）

业务类型唯一事实源是 `packages/contract/openapi.yaml`，经 `openapi-typescript` 生成
`packages/contract/generated/v3.d.ts`，再复制到本目录 `src/api/v3.d.ts`。

再生成方式：

```bash
# 1) 重新生成契约类型
cd ../../packages/contract
npm install
npm run gen        # openapi.yaml → generated/v3.d.ts

# 2) 复制到 apps/web
cd ../../apps/web
npm run gen:api    # 等价于 node scripts/copy-contract-types.mjs
```

前端只通过 `import type { components } from './v3'`（或经 `src/api/endpoints.ts` 的类型别名）
引用这些类型，不手写业务接口。

> 例外：SSE 事件结构（`src/api/events.ts`）不在 OpenAPI 契约内（`/events` 响应未声明 schema），
> 其 `type` 标签联合按 `crates/mimotts-core/src/events.rs` 建模。

## 目录结构

```
src/
├─ api/            # v3.d.ts(生成) · client.ts(fetch 封装) · endpoints.ts · events.ts
├─ stores/         # zustand：theme / auth / config / providers(熔断健康)
├─ hooks/          # useEventSource(重连退避) · useTaskStream(多任务) · useAudioUrl
├─ lib/            # 纯函数：status(状态映射/格式化) · backoff · sse · download (+ vitest 单测)
├─ components/     # Shell / TaskRow / TaskCard / VoiceCard / StatusBadge / ProviderHealthBar …
└─ pages/          # 路由级懒加载：Workbench / Import / TaskList / TaskDetail / Settings
```

## 路由

| 路径 | 页面 |
|---|---|
| `/` | 合成工作台（文本/风格指令/9 音色卡片/模型/标签/Provider/提交进度） |
| `/import` | 批量导入（多文件拖放、编码说明、会话进度 SSE、ZIP 导出） |
| `/tasks` | 任务历史（分页 + status/session/search 过滤 + 虚拟滚动 + SSE 实时） |
| `/tasks/:id` | 任务详情（音频播放/分片列表/重试/取消/删除/文本预览） |
| `/settings` | 设置（API Token / Provider Key / 服务端统计 / 端点参考） |

## 关键实现说明

- **鉴权**：REST 走 `fetch` + `Authorization: Bearer <localStorage token>`（首次在设置页填写）。
  401 时触发顶栏「未授权」提示。
- **SSE 重连 + 鉴权**：`useEventSource` 使用 EventSource，断线按指数退避（1s→30s full jitter）重连。
  URL 一律由 `buildEventUrl()` 拼接：经 `POST /auth/scoped` 换取短期 scoped 凭据后走 `?token=` query
  （`/events` 服务端哈希校验 + Bearer 双通道），原始 API token 从不进 URL。
- **运行时统计**：`/api/v3/stats`（生成类型 `ServerStats`）由 Shell 全局 5s 轮询，顶栏展示
  队列深度/worker 数与 provider 熔断冷却倒计时；设置页同源展示 provider 明细表。
- **音频播放（原生 Range）**：`useAudioUrl` 返回 `/api/v3/tasks/{id}/audio?token=<scoped>` 交给
  原生 `<audio>`，浏览器自动发 Range 实现流式 seek（后端 206）；下载走 `/tasks/{id}/download?token=<scoped>` 直链。
- **音色试听代理**：试听统一走 `/api/v3/voices/{id}/preview?token=<scoped>`（后端 302 → CDN 白名单）；
  试听按钮始终渲染，不依赖 `voice.preview_url`。
- **ZIP 导出**：`/sessions/{id}/export` 契约未声明 token query，仍走 `fetch → Blob`（`lib/download.ts`）。

## 遗留 TODO

1. **vite-plus（可选增强）**：尝试结果见报告；主构建链固定为 `vite` + `@vitejs/plugin-react`
   + `@tailwindcss/vite`，不依赖 vite-plus。
