# UI 增强计划 v3.1

> **Created:** 2026-06-15
> **Status:** Plan — 待审核

## [S1] 需求

用户反馈前端 v3.0 缺乏：
1. **背景图/品牌标识** — 旧版有 "UM-MIMO-TTS" 文字背景，新版缺失
2. **玻璃磨砂卡片** — 现代毛玻璃效果 (glassmorphism)，视觉层次
3. **信息 Footer** — 页面底部版本/版权信息
4. **Provider 配置与选择** — 设置页 Provider 配置已存在，但首页缺乏 Provider 选择入口

## [S2] 设计方向

### 2.1 背景与品牌

**旧版 BrandHero.vue 分析：**
- 实现了背景水印效果
- "UM-MIMO-TTS" 大字 + 毛玻璃卡片 + 标签页切换（控制/配置）
- 这是 v2.0 的一个完整组件

**新版方案：**
- 保留毛玻璃/渐变背景作为全局装饰
- 品牌标题区域：左上角简洁的 Logo + 文字
- 整体风格从左侧栏布局回归到 v2.0 的单页居中布局（更适合 TTS 这种简单应用）

### 2.2 玻璃磨砂效果 (Glassmorphism)

```css
/* 毛玻璃卡片 */
.glass-card {
  background: rgba(255, 255, 255, 0.6);
  backdrop-filter: blur(12px);
  -webkit-backdrop-filter: blur(12px);
  border: 1px solid rgba(255, 255, 255, 0.3);
  box-shadow: 0 8px 32px rgba(0, 0, 0, 0.06);
}

.dark .glass-card {
  background: rgba(30, 30, 30, 0.6);
  border: 1px solid rgba(255, 255, 255, 0.08);
  box-shadow: 0 8px 32px rgba(0, 0, 0, 0.2);
}
```

适用组件：
- SynthesizeForm 卡片
- TaskList 任务项
- Settings Provider 配置卡片

### 2.3 Footer

- 固定在页面底部（非粘滞）
- 左侧：版本号 "v3.0.0"
- 右侧：版权 "2026 UM-MIMO-TTS"
- 使用 `border-t` 分隔

### 2.4 Provider 选择入口

**问题：** 当前只能在 Settings 页面配置 Provider，首页无法选择/切换
**方案：** 在 SynthesizeForm 中增加 Provider 下拉选择器，从 configStore.providers 读取已配置的 Provider

## [S3] 实施计划

### Task 1: 全局背景 + 品牌标题
**Files:** `frontend/src/App.vue`, `frontend/src/style.css`
**Changes:**
- App.vue 添加背景装饰层（渐变圆 + 网格）
- 品牌标题 "UM-MIMO-TTS" 居中显示
- style.css 添加玻璃动画 keyframes

### Task 2: 玻璃磨砂卡片样式
**Files:** `frontend/src/style.css`
**Changes:**
- 添加 `.glass-card` CSS 类
- 支持 light/dark 模式切换

### Task 3: 升级 SynthesizeForm
**Files:** `frontend/src/components/SynthesizeForm.vue`
**Changes:**
- 添加 `glass-card` 类
- 增加 Provider 选择下拉器（已配置的 Provider）
- 整体布局从 Card 改为毛玻璃容器

### Task 4: 升级 TaskList 任务卡片
**Files:** `frontend/src/components/TaskList.vue`
**Changes:**
- 任务卡片添加 `.glass-card` 效果
- 调整间距和圆角

### Task 5: 添加 Footer 组件
**Files:** `frontend/src/components/Footer.vue` (new), `frontend/src/views/Home.vue`, `frontend/src/views/Settings.vue`, `frontend/src/views/TaskDetail.vue`
**Changes:**
- 创建 Footer 组件
- 集成到所有页面

### Task 6: 升级 Settings Provider 配置
**Files:** `frontend/src/views/Settings.vue`
**Changes:**
- Provider 卡片添加玻璃效果
- 布局优化

### Task 7: 构建验证
```bash
cd frontend && npm run build
```

## [S4] 约束

1. **不改变后端 API**
2. **不改变数据库结构**
3. **不改变 Composable/Store 架构**
4. **纯 CSS + Template 调整**
5. **保持暗色模式支持**
