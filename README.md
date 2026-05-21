# MIMO v2.5 TTS Web 服务

![License](https://img.shields.io/badge/license-MIT-blue.svg)
![Rust](https://img.shields.io/badge/Rust-1.70+-orange.svg)
![Vue](https://img.shields.io/badge/Vue-3.5-brightgreen.svg)
![Node](https://img.shields.io/badge/Node.js-24_LTS-brightgreen.svg)
![Version](https://img.shields.io/badge/version-1.0.0-blue.svg)
![Status](https://img.shields.io/badge/status-active-success.svg)

基于 Rust + Actix-web + Vue 3 的小米 MIMO v2.5 TTS 语音合成 Web 服务

**前后端一体化** - 单个可执行文件包含完整应用，无需额外依赖

---

## 🚀 快速开始

### 方式一：下载预编译版本（推荐）

1. 从 [Releases](https://github.com/UnforgetMemory/UMMimoTTS/releases) 下载对应平台的压缩包：

   | 平台 | 文件名 |
   |------|--------|
   | Windows x86_64 | `mimo-tts-server-windows-x86_64.zip` |
   | Linux x86_64 | `mimo-tts-server-linux-x86_64.tar.gz` |
   | macOS Intel | `mimo-tts-server-macos-x86_64.tar.gz` |
   | macOS Apple Silicon | `mimo-tts-server-macos-aarch64.tar.gz` |

2. 解压后直接运行：

   **Windows**：
   ```powershell
   # 解压 zip 文件
   Expand-Archive mimo-tts-server-windows-x86_64.zip
   cd mimo-tts-server-windows-x86_64
   
   # 运行（前端已嵌入，无需额外步骤）
   .\mimo-tts-server.exe
   ```

   **Linux / macOS**：
   ```bash
   # 解压
   tar xzf mimo-tts-server-linux-x86_64.tar.gz
   cd mimo-tts-server-linux-x86_64
   
   # 添加执行权限（Linux/macOS）
   chmod +x mimo-tts-server
   
   # 运行
   ./mimo-tts-server
   ```

3. 打开浏览器访问 http://localhost:30231

4. 在页面上配置你的 MIMO API Key（从 [platform.xiaomimimo.com](https://platform.xiaomimimo.com/) 申请）

### 方式二：从源码编译

#### 环境要求

| 工具 | 版本 | 说明 |
|------|------|------|
| **Rust** | 1.70+ | 后端编译 |
| **Node.js** | 24 LTS | 前端构建 |
| **npm** | 10+ | 随 Node.js 安装 |

#### 编译步骤

```bash
# 克隆仓库
git clone https://github.com/UnforgetMemory/UMMimoTTS.git
cd UMMimoTTS

# 构建前端
cd frontend
npm install
npm run build
cd ..

# 构建后端（自动嵌入前端 dist）
cd backend
cargo build --release

# 运行
./target/release/mimo-tts-server
```

编译产物：
- `backend/target/release/mimo-tts-server` (Linux/macOS)
- `backend/target/release/mimo-tts-server.exe` (Windows)

**注意**：`cargo build` 会自动将 `frontend/dist/` 目录嵌入到二进制文件中，无需手动复制。

---

## 📖 使用说明

### 配置 API Key

首次运行时，需要在 Web 界面上配置 MIMO API Key：

1. 打开 http://localhost:30231
2. 点击右下角的设置图标（⚙️）
3. 输入你的 API Key
4. 点击"保存"

API Key 会保存在浏览器的 localStorage 中，不会上传到服务器。

### 合成语音

1. 在"合成文本"输入框中输入要合成的文本
2. 选择音色（8 种预置音色可选）
3. 可选：输入"风格描述"来控制语音风格
4. 点击"开始合成"
5. 等待合成完成，在任务列表中播放或下载

### 超长文本

系统会自动将超长文本（>2000 字）分割成多个片段分别合成，然后合并为完整的音频文件。进度会实时显示（如"第 2/5 片"）。

### API 限制

| 限制项 | 值 |
|--------|-----|
| 每分钟请求数 (RPM) | 10 |
| 每分钟 Token (TPM) | 1M |
| 单次最大文本长度 | 2000 字（超长文本自动分片）|

---

## 🏗️ 技术架构

### 系统架构

```
┌─────────────────────────────────────────────────────────┐
│                    mimo-tts-server.exe                   │
│  ┌─────────────────────────────────────────────────────┐ │
│  │              Rust (Actix-web) Backend               │ │
│  │  ┌──────────┐ ┌──────────┐ ┌─────────────────────┐ │ │
│  │  │  Tasks   │ │   TTS    │ │    Embedded Frontend │ │ │
│  │  │  API     │ │  Synth   │ │    (Vue 3 + Vite)   │ │ │
│  │  └──────────┘ └──────────┘ └─────────────────────┘ │ │
│  └─────────────────────────────────────────────────────┘ │
└─────────────────────────────────────────────────────────┘
         │                              │
         ▼                              ▼
   MIMO TTS API                   Browser UI
   (Xiaomi Cloud)                 (localhost:30231)
```

### 技术栈

**后端**：
- **Rust** + **Actix-web 4.x** - 高性能异步 Web 框架
- **tokio** - 异步运行时
- **reqwest** - HTTP 客户端
- **rust-embed** - 编译时嵌入前端资源
- **serde** - 序列化/反序列化
- **tracing** - 结构化日志

**前端**：
- **Vue 3.5** + **Vite 6** - 现代化前端框架
- **TypeScript** - 类型安全
- **Tailwind CSS 4** - 原子化 CSS
- **shadcn-vue** - UI 组件库
- **Pinia** - 状态管理

### 核心特性

#### 智能文本分片

超长文本自动按句子边界分割，每片 ≤2000 字，独立合成后合并为完整音频。

```rust
// 分片策略：先算最优片数，再均匀分配
let chunk_count = ceil(total_chars / MAX_CHUNK);
let target_size = total_chars / chunk_count;
```

#### API 流控

滑动窗口速率限制器，确保不超过 API 限制（10 RPM）。

```rust
struct RateLimiter {
    request_times: VecDeque<Instant>,
    max_rpm: usize,  // 10 次/分钟
}
```

#### WAV 音频合并

正确解析 WAV 头格式，拼接多个音频片段的 PCM 数据。

---

## 📡 API 文档

### 基础 URL

```
http://localhost:30231
```

### 端点列表

| 方法 | 路径 | 说明 |
|------|------|------|
| POST | `/api/v1/tts/synthesize` | 创建合成任务 |
| GET | `/api/v1/tasks` | 获取所有任务 |
| GET | `/api/v1/tasks/{id}` | 获取任务详情 |
| DELETE | `/api/v1/tasks/{id}` | 删除任务 |
| GET | `/api/v1/tasks/{id}/audio` | 下载音频（支持 Range） |
| PATCH | `/api/v1/tasks/{id}/title` | 更新任务标题 |
| GET | `/api/v1/voices` | 获取音色列表 |
| GET | `/api/v1/sse/tasks/{id}` | 任务状态 SSE 推送 |
| GET | `/health` | 健康检查 |

### 创建合成任务

**请求**：
```bash
curl -X POST http://localhost:30231/api/v1/tts/synthesize \
  -H "Content-Type: application/json" \
  -d '{
    "text": "你好，世界",
    "voice": "zhifeng_16k",
    "model": "mimo-v2.5-tts",
    "api_key": "your_api_key_here"
  }'
```

**响应**（200 OK）：
```json
{
  "task_id": "019e493e-a76d-78d2-a0fd-2af9731a101d",
  "status": "pending",
  "token_count": 8,
  "char_count": 5,
  "message": "任务已创建，正在合成中"
}
```

### 音频下载

支持 HTTP Range 请求，可用于进度条 seek：

```bash
# 完整下载
curl -O http://localhost:30231/api/v1/tasks/{id}/audio

# 部分下载
curl -H "Range: bytes=0-1023" http://localhost:30231/api/v1/tasks/{id}/audio
```

---

## 🛠️ 开发指南

### 项目结构

```
UMMimoTTS/
├── backend/
│   ├── src/
│   │   ├── main.rs           # 入口文件
│   │   ├── embed.rs          # 前端嵌入模块
│   │   ├── routes/           # API 路由
│   │   ├── services/         # 业务逻辑
│   │   ├── models/           # 数据模型
│   │   └── state/            # 应用状态
│   └── Cargo.toml
├── frontend/
│   ├── src/
│   │   ├── components/       # Vue 组件
│   │   ├── stores/           # Pinia 状态
│   │   └── api/              # API 客户端
│   └── package.json
├── .github/workflows/        # CI/CD
└── README.md
```

### 开发模式

```bash
# 终端 1：启动后端（开发模式，不嵌入前端）
cd backend
cargo run

# 终端 2：启动前端（开发服务器，支持热更新）
cd frontend
npm run dev -- --port 30232
```

访问 http://localhost:30232 使用前端开发服务器。

### 运行测试

```bash
# 后端测试
cd backend
cargo test

# 前端类型检查
cd frontend
npm run build
```

### 环境变量

| 变量名 | 说明 | 默认值 |
|--------|------|--------|
| `MIMO_API_KEY` | MIMO API 密钥（可选，可在 Web 界面配置） | 无 |
| `MAX_CONCURRENT_TASKS` | 最大并发任务数 | 5 |
| `TASK_CLEANUP_HOURS` | 任务清理时间（小时） | 24 |
| `RUST_LOG` | 日志级别 | `mimo_tts_server=info,actix_web=info` |

---

## 📦 构建与发布

### CI/CD

项目使用 GitHub Actions 自动构建和发布：

- **CI**：每次推送到 `main` 分支时运行测试
- **Release**：推送 `v*` 标签时自动构建多平台二进制文件并创建 Release

### 手动构建 Release

```bash
# 构建前端
cd frontend
npm run build
cd ..

# 构建后端（自动嵌入前端）
cd backend
cargo build --release

# 打包（Linux/macOS）
tar czf mimo-tts-server-linux-x86_64.tar.gz -C target/release mimo-tts-server

# 打包（Windows）
Compress-Archive -Path target/release/mimo-tts-server.exe -DestinationPath mimo-tts-server-windows-x86_64.zip
```

---

## 📋 更新日志

### v1.0.0 (2026-05-21)

**新功能**
- 🎉 前后端一体化：单个可执行文件包含完整应用
- ✨ 智能文本分片合成（支持超长文本）
- ✨ WAV 音频自动合并
- ✨ API 流控机制（10 RPM 滑动窗口）
- ✨ HTTP Range 支持（音频 seek）
- ✨ Card 布局任务列表
- ✨ API Key 占位符自动检测

**修复**
- 🐛 音频播放器状态监听（play/pause 事件驱动）
- 🐛 时间显示浮点数溢出
- 🐛 WAV 合并音频嘈杂（头解析错误）
- 🐛 分片合并文本溢出

**优化**
- ⚡ RateLimiter Vec → VecDeque
- ⚡ WAV 合并预分配缓冲区
- ⚡ 避免重复分片计算

---

## 📄 许可证

MIT License - 详见 [LICENSE](LICENSE)

## 🙏 致谢

- [MIMO](https://platform.xiaomimimo.com/) - 小米 TTS API
- [Actix-web](https://actix.rs/) - Rust Web 框架
- [Vue.js](https://vuejs.org/) - 前端框架
- [shadcn-vue](https://www.shadcn-vue.com/) - UI 组件库
- [rust-embed](https://github.com/pyros2097/rust-embed) - 静态资源嵌入
