import { describe, it, expect, vi, beforeEach } from 'vitest'
import { render, screen, waitFor } from '@testing-library/react'
import userEvent from '@testing-library/user-event'
import TaskDetailPage from './TaskDetailPage'
import { renderWithRouter } from '@/test/utils'

// —— Module mocks ——
vi.mock('@/api/endpoints', () => ({
  fetchTask: vi.fn(),
  retryTask: vi.fn(),
  cancelTask: vi.fn(),
  deleteTask: vi.fn(),
  taskDownloadUrl: vi.fn(),
}))

vi.mock('@/hooks/useEventSource', () => ({
  useEventSource: vi.fn(),
}))

vi.mock('@/hooks/useAudioUrl', () => ({
  useAudioUrl: vi.fn(() => ({ url: null })),
}))

vi.mock('@/stores/auth', () => ({
  useAuthStore: vi.fn(),
}))

// Mock react-router hooks: renderWithRouter only provides a MemoryRouter
// context and no route rules, so useParams returns undefined. Force
// { id: 't1' } here.
vi.mock('react-router', async () => {
  const actual = await vi.importActual<typeof import('react-router')>('react-router')
  return {
    ...actual,
    useParams: vi.fn(() => ({ id: 't1' })),
    useNavigate: vi.fn(() => vi.fn()),
  }
})

import { fetchTask, retryTask, cancelTask, deleteTask, taskDownloadUrl } from '@/api/endpoints'
import { useAudioUrl } from '@/hooks/useAudioUrl'
import { useAuthStore } from '@/stores/auth'

const mockedFetchTask = vi.mocked(fetchTask)
const mockedRetryTask = vi.mocked(retryTask)
const mockedCancelTask = vi.mocked(cancelTask)
const mockedDeleteTask = vi.mocked(deleteTask)
const mockedTaskDownloadUrl = vi.mocked(taskDownloadUrl)
const mockedUseAudioUrl = vi.mocked(useAudioUrl)
const mockedUseAuthStore = vi.mocked(useAuthStore)

const baseTask = {
  id: 't1',
  title: '测试任务',
  status: 'done' as const,
  voice: 'v1',
  model: 'm1',
  style: null,
  session_id: 's1',
  total_chunks: 2,
  done_chunks: 2,
  failed_chunks: 0,
  duration_ms: 5000,
  error: null,
  has_audio: true,
  content: '你好，世界。',
  chunks: [
    { id: 'c1', task_id: 't1', seq: 1, text: '你好，', token_estimate: 5, status: 'done' as const, retry_count: 0, duration_ms: 2000 },
    { id: 'c2', task_id: 't1', seq: 2, text: '世界。', token_estimate: 5, status: 'done' as const, retry_count: 0, duration_ms: 3000 },
  ],
  created_at: '2026-01-01T00:00:00Z',
  completed_at: '2026-01-01T00:00:05Z',
  total_chars: 6,
  total_tokens: 10,
} as any

beforeEach(() => {
  vi.clearAllMocks()
  mockedUseAuthStore.mockImplementation((selector: any) => selector({ token: 'tk' }))
  mockedUseAudioUrl.mockReturnValue({ url: null })
  mockedTaskDownloadUrl.mockResolvedValue('/api/v3/tasks/t1/download?token=tk')
})

describe('TaskDetailPage', () => {
  it('加载时显示 Spinner', () => {
    mockedFetchTask.mockImplementation(() => new Promise(() => {}))

    renderWithRouter(<TaskDetailPage />)

    expect(document.querySelector('.animate-spin')).toBeInTheDocument()
  })

  it('加载失败显示错误', async () => {
    mockedFetchTask.mockRejectedValueOnce(new Error('任务不存在'))

    renderWithRouter(<TaskDetailPage />)

    await screen.findByText('任务不存在')
  })

  it('正常加载显示任务详情', async () => {
    mockedFetchTask.mockResolvedValueOnce(baseTask)

    renderWithRouter(<TaskDetailPage />)

    await screen.findByText('测试任务')
    expect(screen.getByText('t1')).toBeInTheDocument()
  })

  it('显示任务标题和 ID', async () => {
    mockedFetchTask.mockResolvedValueOnce(baseTask)

    renderWithRouter(<TaskDetailPage />)

    await screen.findByText('测试任务')
    expect(screen.getByText('t1')).toBeInTheDocument()
  })

  it('显示进度条', async () => {
    mockedFetchTask.mockResolvedValueOnce(baseTask)

    renderWithRouter(<TaskDetailPage />)

    await screen.findByText(/分片 2\/2/)
  })

  it('有音频时显示 audio 元素和下载按钮', async () => {
    mockedFetchTask.mockResolvedValueOnce(baseTask)
    mockedUseAudioUrl.mockReturnValue({ url: 'http://example.com/audio.wav' })

    renderWithRouter(<TaskDetailPage />)

    await screen.findByTestId('task-detail-audio')
    await screen.findByText('下载')
  })

  it('无音频时显示提示', async () => {
    mockedFetchTask.mockResolvedValueOnce({ ...baseTask, has_audio: false })
    mockedUseAudioUrl.mockReturnValue({ url: null })

    renderWithRouter(<TaskDetailPage />)

    await screen.findByText('音频尚未就绪')
  })

  it('failed 状态显示重试按钮', async () => {
    mockedFetchTask.mockResolvedValueOnce({ ...baseTask, status: 'failed' })

    renderWithRouter(<TaskDetailPage />)

    await screen.findByText('重试')
  })

  it('非终态显示取消按钮', async () => {
    mockedFetchTask.mockResolvedValueOnce({ ...baseTask, status: 'pending' })

    renderWithRouter(<TaskDetailPage />)

    await screen.findByText('取消')
  })

  it('始终显示删除按钮', async () => {
    mockedFetchTask.mockResolvedValueOnce(baseTask)

    renderWithRouter(<TaskDetailPage />)

    await screen.findByText('删除')
  })

  it('显示分片列表', async () => {
    mockedFetchTask.mockResolvedValueOnce(baseTask)

    renderWithRouter(<TaskDetailPage />)

    await screen.findByText(/分片列表（2）/)
  })

  it('重试按钮调用 retryTask', async () => {
    const user = userEvent.setup()
    mockedFetchTask.mockResolvedValueOnce({ ...baseTask, status: 'failed' })
    mockedRetryTask.mockResolvedValueOnce(undefined)

    renderWithRouter(<TaskDetailPage />)

    const retryBtn = await screen.findByText('重试')
    await user.click(retryBtn)

    expect(mockedRetryTask).toHaveBeenCalledWith('t1')
  })

  it('取消按钮调用 cancelTask', async () => {
    const user = userEvent.setup()
    mockedFetchTask.mockResolvedValueOnce({ ...baseTask, status: 'pending' })
    mockedCancelTask.mockResolvedValueOnce(undefined)

    renderWithRouter(<TaskDetailPage />)

    const cancelBtn = await screen.findByText('取消')
    await user.click(cancelBtn)

    expect(mockedCancelTask).toHaveBeenCalledWith('t1')
  })

  it('删除按钮调用 deleteTask', async () => {
    const user = userEvent.setup()
    mockedFetchTask.mockResolvedValueOnce(baseTask)
    mockedDeleteTask.mockResolvedValueOnce(undefined)

    renderWithRouter(<TaskDetailPage />)

    const deleteBtn = await screen.findByText('删除')
    await user.click(deleteBtn)

    expect(mockedDeleteTask).toHaveBeenCalledWith('t1')
  })

  it('显示任务元信息（音色、模型、风格）', async () => {
    mockedFetchTask.mockResolvedValueOnce({
      ...baseTask,
      style: '温柔语气',
      session_id: 's1',
    })

    renderWithRouter(<TaskDetailPage />)

    await screen.findByText(/音色：v1/)
    expect(screen.getByText(/模型：m1/)).toBeInTheDocument()
    expect(screen.getByText(/风格：温柔语气/)).toBeInTheDocument()
  })

  it('显示错误信息', async () => {
    mockedFetchTask.mockResolvedValueOnce({ ...baseTask, error: '合成失败' })

    renderWithRouter(<TaskDetailPage />)

    await screen.findByText('合成失败')
  })

  it('显示文本预览', async () => {
    mockedFetchTask.mockResolvedValueOnce({ ...baseTask, content: '测试内容文本' })

    renderWithRouter(<TaskDetailPage />)

    await screen.findByText('测试内容文本')
  })

  it('分片列表为空时显示占位', async () => {
    mockedFetchTask.mockResolvedValueOnce({ ...baseTask, chunks: [] })

    renderWithRouter(<TaskDetailPage />)

    await screen.findByText('暂无分片')
  })
})
