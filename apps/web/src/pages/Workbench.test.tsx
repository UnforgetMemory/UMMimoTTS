import { describe, it, expect, vi, beforeEach } from 'vitest'
import { render, screen } from '@testing-library/react'
import userEvent from '@testing-library/user-event'
import Workbench from './Workbench'

// —— Module mocks ——
vi.mock('@/stores/config', () => ({
  useConfigStore: vi.fn(),
}))

vi.mock('@/api/endpoints', () => ({
  createTask: vi.fn(),
  voicePreviewUrl: vi.fn(),
}))

vi.mock('@/lib/status', () => ({
  extractTitle: vi.fn(() => '测试标题'),
}))

vi.mock('@/components/VoiceCard', () => ({
  VoiceCard: vi.fn(() => null),
}))

vi.mock('@/components/TaskCard', () => ({
  TaskCard: vi.fn(() => null),
}))

vi.mock('@/hooks/useEventSource', () => ({
  useEventSource: vi.fn(),
}))

import { useConfigStore } from '@/stores/config'
import { createTask, voicePreviewUrl } from '@/api/endpoints'
import { extractTitle } from '@/lib/status'
import { VoiceCard } from '@/components/VoiceCard'
import { TaskCard } from '@/components/TaskCard'

const mockedUseConfigStore = vi.mocked(useConfigStore)
const mockedCreateTask = vi.mocked(createTask)
const mockedExtractTitle = vi.mocked(extractTitle)
const mockedVoiceCard = vi.mocked(VoiceCard)

const defaultConfig = {
  voices: [{ id: 'v1', name: '小美', language: 'zh', gender: 'female' }],
  models: [{ id: 'm1', name: 'Mimo v2.5' }],
  providers: [{ id: 'p1', name: '默认', base_url: 'https://x', kind: 'xiaomi' as const, is_configured: true, is_default: true }],
  default_voice: 'v1',
  default_model: 'm1',
  chunk: {},
} as any

function mockConfig(config = defaultConfig, error: string | null = null) {
  mockedUseConfigStore.mockImplementation((selector: any) => {
    const store = { load: vi.fn().mockResolvedValue(config), config, error, loading: false, reset: vi.fn() }
    return selector(store)
  })
}

function renderWithRouter(ui: React.ReactElement) {
  return render(
    <TestRouter>
      {ui}
    </TestRouter>,
  )
}

function TestRouter({ children }: { children: React.ReactNode }) {
  return children
}

beforeEach(() => {
  vi.clearAllMocks()
  mockedCreateTask.mockReset()
  mockedExtractTitle.mockReturnValue('测试标题')
})

describe('Workbench', () => {
  it('渲染合成工作台标题', () => {
    mockConfig()
    renderWithRouter(<Workbench />)
    expect(screen.getByText('合成工作台')).toBeInTheDocument()
  })

  it('渲染文本输入框', () => {
    mockConfig()
    renderWithRouter(<Workbench />)
    expect(screen.getByTestId('workbench-content')).toBeInTheDocument()
  })

  it('渲染音频标签按钮', () => {
    mockConfig()
    renderWithRouter(<Workbench />)
    expect(screen.getByText('[笑]')).toBeInTheDocument()
    expect(screen.getByText('[吸气]')).toBeInTheDocument()
    expect(screen.getByText('[语速加快]')).toBeInTheDocument()
    expect(screen.getByText('(唱歌)')).toBeInTheDocument()
  })

  it('渲染模型选择', () => {
    mockConfig()
    renderWithRouter(<Workbench />)
    expect(screen.getByLabelText('模型')).toBeInTheDocument()
  })

  it('渲染提交按钮', () => {
    mockConfig()
    renderWithRouter(<Workbench />)
    expect(screen.getByTestId('workbench-submit')).toBeInTheDocument()
    expect(screen.getByTestId('workbench-submit')).toHaveTextContent('开始合成')
  })

  it('空文本提交时显示错误', async () => {
    const user = userEvent.setup()
    mockConfig()
    renderWithRouter(<Workbench />)

    await user.click(screen.getByTestId('workbench-submit'))

    expect(screen.getByText('请输入待合成文本')).toBeInTheDocument()
  })

  it('无音色提交时显示错误', async () => {
    const user = userEvent.setup()
    mockConfig({ ...defaultConfig, voices: [], default_voice: '' } as any)
    renderWithRouter(<Workbench />)

    // type the content
    await user.type(screen.getByTestId('workbench-content'), '你好世界')

    // click submit
    await user.click(screen.getByTestId('workbench-submit'))

    expect(screen.getByText('请选择音色')).toBeInTheDocument()
  })

  it('正常提交时调用 createTask', async () => {
    const user = userEvent.setup()
    const mockTask = {
      id: 't1',
      title: '测试',
      status: 'pending',
      voice: 'v1',
      model: 'm1',
      total_chunks: 1,
      done_chunks: 0,
      failed_chunks: 0,
      has_audio: false,
      created_at: '2026-01-01T00:00:00Z',
    } as any

    mockedCreateTask.mockResolvedValueOnce(mockTask)

    mockConfig()
    renderWithRouter(<Workbench />)

    await user.type(screen.getByTestId('workbench-content'), '你好世界')
    await user.click(screen.getByTestId('workbench-submit'))

    expect(mockedCreateTask).toHaveBeenCalled()
    expect(mockedCreateTask.mock.calls[0][0]).toMatchObject({
      content: '你好世界',
      voice: 'v1',
      model: 'm1',
    })
  })

  it('提交成功后清空内容', async () => {
    const user = userEvent.setup()
    mockedCreateTask.mockResolvedValueOnce({
      id: 't1',
      title: '测试',
      status: 'pending',
      voice: 'v1',
      model: 'm1',
      total_chunks: 1,
      done_chunks: 0,
      failed_chunks: 0,
      has_audio: false,
      created_at: '2026-01-01T00:00:00Z',
    } as any)

    mockConfig()
    renderWithRouter(<Workbench />)

    const textarea = screen.getByTestId('workbench-content')
    await user.type(textarea, '你好世界')

    await user.click(screen.getByTestId('workbench-submit'))

    // wait for the async work
    await new Promise((r) => setTimeout(r, 0))

    expect(textarea).toHaveValue('')
  })

  it('配置加载错误显示', () => {
    mockConfig(null, '网络错误')
    renderWithRouter(<Workbench />)
    expect(screen.getByText(/加载配置失败/)).toBeInTheDocument()
  })

  it('点击音频标签按钮插入文本', async () => {
    const user = userEvent.setup()
    mockConfig()
    renderWithRouter(<Workbench />)

    const textarea = screen.getByTestId('workbench-content')
    await user.click(screen.getByText('[笑]'))

    expect(textarea).toHaveValue('[笑]')
  })

  it('提交失败时显示错误信息', async () => {
    const user = userEvent.setup()
    mockedCreateTask.mockRejectedValueOnce(new Error('服务器错误'))

    mockConfig()
    renderWithRouter(<Workbench />)

    await user.type(screen.getByTestId('workbench-content'), '你好世界')
    await user.click(screen.getByTestId('workbench-submit'))

    expect(screen.getByText('服务器错误')).toBeInTheDocument()
  })
})
