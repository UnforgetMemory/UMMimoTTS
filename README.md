# MIMO v2.5 TTS Web 服务

基于 Rust + Actix-web + Vue 3 的小米 MIMO v2.5 TTS 语音合成 Web 服务

## 功能特性

- ✅ 完整的 TTS 合成工作流（文本 → 音频）
- ✅ 多任务并发管理与状态追踪
- ✅ 实时任务状态展示
- ✅ 音频在线播放与下载
- ✅ Token/字符实时统计
- ✅ 8 种预置音色切换
- ✅ 自然语言风格控制
- ✅ 完善的错误处理与重试
- ✅ 响应式现代化 UI
- ✅ 内存安全的异步 Rust 后端

## 技术栈

### 后端
- **Rust** + **Actix-web 4.x** - 高性能异步 Web 框架
- **tokio** - 异步运行时
- **reqwest** - HTTP 客户端（调用 MIMO API）
- **parking_lot** - 高性能并发原语
- **serde** - 序列化/反序列化

### 前端
- **Vue 3.5** + **Vite 6** - 现代化前端框架
- **TypeScript** - 类型安全
- **Tailwind CSS** - 原子化 CSS
- **Axios** - HTTP 客户端
- **Pinia** - 状态管理

## 快速开始

### 前置要求

- Rust 1.70+ (后端)
- Node.js 18+ (前端)
- MIMO API Key ([申请地址](https://platform.xiaomimimo.com/))

### 后端启动

```bash
cd backend

# 复制环境变量配置
cp .env.example .env

# 编辑 .env 文件，填入你的 MIMO_API_KEY
# MIMO_API_KEY=your_api_key_here

# 运行服务器
cargo run

# 或使用 watch 模式（需要 cargo-watch）
cargo watch -x run
```

后端服务将启动在 `http://localhost:30231`

### 前端启动

```bash
cd frontend

# 安装依赖
npm install

# 开发模式
npm run dev

# 生产构建
npm run build
```

前端开发服务器将启动在 `http://localhost:5173`

### 生产部署

```bash
# 构建前端
cd frontend
npm run build

# 后端服务静态文件
cd ../backend
cargo run --release
```

访问 `http://localhost:30231` 即可使用完整应用。

## API 文档

### TTS 合成

```http
POST /api/v1/tts/synthesize
Content-Type: application/json

{
  "text": "你好，世界",
  "voice": "冰糖",
  "model": "mimo-v2.5-tts",
  "context": "用温柔的语气",
  "api_key": "可选，覆盖默认 API Key"
}
```

### 任务管理

```http
GET /api/v1/tasks                    # 获取所有任务
GET /api/v1/tasks/{task_id}          # 获取任务详情
DELETE /api/v1/tasks/{task_id}       # 删除任务
GET /api/v1/tasks/{task_id}/audio    # 下载音频
```

### 音色列表

```http
GET /api/v1/voices
```

### 健康检查

```http
GET /health
```

## 预置音色

| 音色 | 语言 | 性别 | 风格 |
|------|------|------|------|
| 冰糖 | 中文 | 女性 | 活泼少女 |
| 茉莉 | 中文 | 女性 | 知性女声 |
| 苏打 | 中文 | 男性 | 阳光少年 |
| 白桦 | 中文 | 男性 | 成熟男声 |
| Mia | English | Female | Lively girl |
| Chloe | English | Female | Sweet Dreamy |
| Milo | English | Male | Sunny boy |
| Dean | English | Male | Steady Gentle |

## 项目结构

```
UMMimoTTS/
├── backend/                 # Rust 后端
│   ├── src/
│   │   ├── main.rs          # 服务器入口
│   │   ├── config.rs        # 配置管理
│   │   ├── models/          # 数据模型
│   │   ├── services/        # 业务逻辑
│   │   ├── routes/          # API 路由
│   │   └── state/           # 应用状态
│   └── Cargo.toml
└── frontend/                # Vue 前端
    ├── src/
    │   ├── App.vue          # 主应用组件
    │   ├── api/             # API 客户端
    │   └── ...
    └── package.json
```

## 环境变量

| 变量 | 描述 | 默认值 |
|------|------|--------|
| `MIMO_API_KEY` | MIMO API 密钥 | (空) |
| `SERVER_PORT` | 服务器端口 | `30231` |
| `ALLOWED_ORIGINS` | CORS 白名单 | `http://localhost:5173` |
| `MAX_CONCURRENT_TASKS` | 最大并发任务数 | `5` |
| `TASK_CLEANUP_HOURS` | 任务清理时间（小时） | `24` |

## 开发说明

### 任务状态流转

```
Pending → Queued → Synthesizing → Streaming → Completed
                  ↓
                Failed (可重试)
```

### 并发控制

使用信号量（Semaphore）限制同时合成的任务数，避免对 MIMO API 造成过大压力。

### 错误处理

- 400: 请求参数错误
- 401: API Key 无效
- 429: 请求频率限制
- 500: 服务端错误

## 许可证

本项目采用 **MIT License** 开源协议。

详见 [LICENSE](LICENSE) 文件。

## 第三方资源声明

### 小米官方资源引用

本项目使用了以下来自小米官方的资源和材料：

1. **音色预览音频**
   - 来源：小米 MIMO TTS 官方 CDN
   - URL: `https://aistudio-cdn.xiaomimimo.com/xiaomimimo-static/tts/audio/`
   - 包含音色：冰糖、茉莉、苏打、白桦、Mia、Chloe、Milo、Dean
   - 版权：归小米公司所有

2. **音色头像图片**
   - 来源：小米 MiMo Studio 官方网站
   - 格式：WebP
   - 版权：归小米公司所有

3. **API 服务**
   - 服务提供方：小米 MIMO 开放平台
   - 文档：[MIMO API 文档](https://platform.xiaomimimo.com/docs/zh-CN/usage-guide/speech-synthesis-v2.5)
   - 使用需遵守小米开放平台服务条款

### 免责声明

- 本项目仅为技术演示和学习用途，非小米官方产品
- 所有音色相关资源（音频、图片）的版权归小米公司所有
- 使用本项目时需自行申请 MIMO API Key 并遵守小米开放平台的使用条款
- 本项目作者与小米公司无任何隶属关系
- 如因使用本项目产生的任何纠纷，由使用者自行承担

### 致谢

感谢小米公司提供优质的 TTS 服务和开发平台。

## 相关链接

- [MIMO 开放平台](https://platform.xiaomimimo.com/)
- [MIMO API 文档](https://platform.xiaomimimo.com/docs/zh-CN/usage-guide/speech-synthesis-v2.5)
- [MiMo-Skills GitHub](https://github.com/XiaomiMiMo/MiMo-Skills)
