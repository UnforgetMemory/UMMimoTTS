import { describe, it, expect, vi, beforeEach } from 'vitest'
import { render, screen, waitFor } from '@testing-library/react'
import userEvent from '@testing-library/user-event'
import SettingsPage from './SettingsPage'
import { renderWithRouter } from '@/test/utils'

// —— Module mocks ——
vi.mock('@/stores/auth', () => ({
  useAuthStore: vi.fn(),
}))

vi.mock('@/stores/config', () => ({
  useConfigStore: vi.fn(),
}))

vi.mock('@/stores/stats', () => ({
  useStatsStore: vi.fn(),
}))

vi.mock('@/api/endpoints', () => ({
  fetchProviders: vi.fn(),
  saveProviderKey: vi.fn(),
  setDefaultProvider: vi.fn(),
  updateProvider: vi.fn(),
}))

vi.mock('@/lib/stats', () => ({
  queueDepth: vi.fn(),
  workerCount: vi.fn(),
}))

import { useAuthStore } from '@/stores/auth'
import { useConfigStore } from '@/stores/config'
import { useStatsStore } from '@/stores/stats'
import { fetchProviders, saveProviderKey, setDefaultProvider, updateProvider } from '@/api/endpoints'
import { queueDepth, workerCount } from '@/lib/stats'

const mockedUseAuthStore = vi.mocked(useAuthStore)
const mockedUseConfigStore = vi.mocked(useConfigStore)
const mockedUseStatsStore = vi.mocked(useStatsStore)
const mockedFetchProviders = vi.mocked(fetchProviders)
const mockedSaveProviderKey = vi.mocked(saveProviderKey)
const mockedSetDefaultProvider = vi.mocked(setDefaultProvider)
const mockedUpdateProvider = vi.mocked(updateProvider)
const mockedQueueDepth = vi.mocked(queueDepth)
const mockedWorkerCount = vi.mocked(workerCount)

const mockProvider = {
  id: 'p1',
  name: '小米',
  base_url: 'https://api.mimo.ai',
  kind: 'xiaomi' as const,
  is_configured: true,
  is_default: true,
  budget_group: 'default',
} as any

function mockAuth(token: string | null = 'tk123') {
  mockedUseAuthStore.mockImplementation((selector: any) => {
    const store = { token, setToken: vi.fn(), clearToken: vi.fn() }
    return selector(store)
  })
}

function mockConfig(config: any = null) {
  mockedUseConfigStore.mockImplementation((selector: any) => {
    const store = { config, loading: false, error: null, load: vi.fn(), reset: vi.fn() }
    return selector(store)
  })
}

function mockStats(stats: any = null, error: string | null = null) {
  mockedUseStatsStore.mockImplementation((selector: any) => {
    const store = { stats, receivedAt: 0, loading: false, error, refresh: vi.fn().mockResolvedValue(undefined) }
    return selector(store)
  })
}

beforeEach(() => {
  vi.clearAllMocks()
  mockAuth()
  mockConfig()
  mockStats()
  mockedFetchProviders.mockResolvedValue([])
  mockedQueueDepth.mockReturnValue(0)
  mockedWorkerCount.mockReturnValue(0)
})

describe('SettingsPage', () => {
  it('渲染 API Token 卡片', () => {
    renderWithRouter(<SettingsPage />)
    expect(screen.getByText('API Token（本地鉴权）')).toBeInTheDocument()
  })

  it('渲染 Token 输入框', () => {
    renderWithRouter(<SettingsPage />)
    expect(screen.getByTestId('settings-token-input')).toBeInTheDocument()
  })

  it('保存 Token 按钮工作', async () => {
    const user = userEvent.setup()
    const setToken = vi.fn()
    mockedUseAuthStore.mockImplementation((selector: any) => {
      const store = { token: null, setToken, clearToken: vi.fn() }
      return selector(store)
    })

    renderWithRouter(<SettingsPage />)

    await user.type(screen.getByTestId('settings-token-input'), 'new-token')
    await user.click(screen.getByText('保存 Token'))

    expect(setToken).toHaveBeenCalledWith('new-token')
    expect(screen.getByText('API Token 已保存到本地')).toBeInTheDocument()
  })

  it('有 token 时显示清除按钮', () => {
    mockAuth('tk123')
    renderWithRouter(<SettingsPage />)

    expect(screen.getByText('清除')).toBeInTheDocument()
  })

  it('无 token 时不显示清除按钮', () => {
    mockAuth(null)
    renderWithRouter(<SettingsPage />)

    expect(screen.queryByText('清除')).not.toBeInTheDocument()
  })

  it('渲染供应商卡片', () => {
    renderWithRouter(<SettingsPage />)
    expect(screen.getByText('供应商（Provider）')).toBeInTheDocument()
  })

  it('供应商为空时显示占位', async () => {
    mockedFetchProviders.mockResolvedValue([])
    renderWithRouter(<SettingsPage />)

    await screen.findByText('暂无供应商配置')
  })

  it('供应商有数据时渲染列表', async () => {
    mockedFetchProviders.mockResolvedValue([mockProvider])
    renderWithRouter(<SettingsPage />)

    await screen.findByText('小米')
    expect(screen.getByText('xiaomi')).toBeInTheDocument()
  })

  it('渲染服务端统计', () => {
    renderWithRouter(<SettingsPage />)
    expect(screen.getByText('服务端统计（/stats）')).toBeInTheDocument()
  })

  it('统计为空时显示加载中', () => {
    mockStats(null)
    renderWithRouter(<SettingsPage />)

    expect(screen.getByText('统计加载中…')).toBeInTheDocument()
  })

  it('统计有数据时显示队列深度和 Worker 数', async () => {
    mockStats({ queue_depth: 5, workers: 8, providers: [] } as any)
    mockedQueueDepth.mockReturnValue(5)
    mockedWorkerCount.mockReturnValue(8)
    renderWithRouter(<SettingsPage />)

    expect(screen.getByText('队列深度')).toBeInTheDocument()
    expect(screen.getByText('5')).toBeInTheDocument()
    expect(screen.getByText('Worker 数')).toBeInTheDocument()
    expect(screen.getByText('8')).toBeInTheDocument()
  })

  it('渲染 API 端点参考表', () => {
    renderWithRouter(<SettingsPage />)
    expect(screen.getByText('API 端点参考（对照 OpenAPI 契约）')).toBeInTheDocument()
  })

  it('分片设置显示', () => {
    mockConfig({
      chunk: { context_window_tokens: 4000, target_tokens: 3000, hard_cap_tokens: 5000 },
    } as any)

    renderWithRouter(<SettingsPage />)

    expect(screen.getByText('分片设置（服务端下发）')).toBeInTheDocument()
    expect(screen.getByText('上下文窗口')).toBeInTheDocument()
    expect(screen.getByText('4000')).toBeInTheDocument()
    expect(screen.getByText('目标 Token')).toBeInTheDocument()
    expect(screen.getByText('3000')).toBeInTheDocument()
    expect(screen.getByText('硬上限 Token')).toBeInTheDocument()
    expect(screen.getByText('5000')).toBeInTheDocument()
  })

  it('供应商编辑表单', async () => {
    const user = userEvent.setup()
    mockedFetchProviders.mockResolvedValue([mockProvider])

    renderWithRouter(<SettingsPage />)

    await screen.findByText('小米')

    await user.click(screen.getByText('编辑'))

    expect(screen.getByPlaceholderText('供应商名称')).toBeInTheDocument()
    expect(screen.getByPlaceholderText('https://…')).toBeInTheDocument()
    expect(screen.getByPlaceholderText('default')).toBeInTheDocument()
  })

  it('保存 API Key', async () => {
    const user = userEvent.setup()
    mockedFetchProviders.mockResolvedValue([mockProvider])
    mockedSaveProviderKey.mockResolvedValueOnce(undefined)

    renderWithRouter(<SettingsPage />)

    await screen.findByText('小米')

    // type an API Key (placeholder "输入新 Key 覆盖保存")
    const keyInput = screen.getByPlaceholderText('输入新 Key 覆盖保存')
    await user.type(keyInput, 'new-api-key')

    // click 保存 Key
    await user.click(screen.getByText('保存 Key'))

    expect(mockedSaveProviderKey).toHaveBeenCalledWith('p1', 'new-api-key')
  })

  it('设为默认供应商', async () => {
    const user = userEvent.setup()
    mockedFetchProviders.mockResolvedValue([{ ...mockProvider, is_default: false }])
    mockedSetDefaultProvider.mockResolvedValueOnce(undefined)

    renderWithRouter(<SettingsPage />)

    await screen.findByText('小米')

    await user.click(screen.getByText('设为默认'))

    expect(mockedSetDefaultProvider).toHaveBeenCalledWith('p1')
  })

  it('显示配置状态', async () => {
    mockedFetchProviders.mockResolvedValue([
      { ...mockProvider, is_configured: true },
      { ...mockProvider, id: 'p2', name: '未配置供应商', is_configured: false, is_default: false },
    ])

    renderWithRouter(<SettingsPage />)

    await screen.findByText('小米')

    expect(screen.getByText('已配置')).toBeInTheDocument()
    expect(screen.getByText('未配置')).toBeInTheDocument()
  })

  it('显示当前 token 状态', () => {
    mockAuth('tk123')
    renderWithRouter(<SettingsPage />)

    expect(screen.getByText(/当前状态：已配置/)).toBeInTheDocument()
  })

  it('无 token 时显示未配置状态', () => {
    mockAuth(null)
    renderWithRouter(<SettingsPage />)

    expect(screen.getByText('当前状态：未配置')).toBeInTheDocument()
  })

  it('统计加载失败显示错误', () => {
    mockStats(null, '网络错误')
    renderWithRouter(<SettingsPage />)

    expect(screen.getByText(/统计加载失败：网络错误/)).toBeInTheDocument()
  })
})
