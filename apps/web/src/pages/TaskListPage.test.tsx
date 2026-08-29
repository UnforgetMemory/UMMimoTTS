import { describe, it, expect, vi, beforeEach } from 'vitest'
import { render, screen, waitFor } from '@testing-library/react'
import userEvent from '@testing-library/user-event'
import TaskListPage from './TaskListPage'
import { renderWithRouter } from '@/test/utils'

// —— Module mocks ——
vi.mock('@/api/endpoints', () => ({
  fetchTasks: vi.fn(),
  fetchSessions: vi.fn(),
}))

vi.mock('@/hooks/useTaskStream', () => ({
  useTaskStream: vi.fn(),
}))

vi.mock('@/lib/status', () => ({
  isActiveTaskStatus: vi.fn(() => false),
}))

vi.mock('@/components/TaskRow', () => ({
  TASK_ROW_HEIGHT: 60,
  TaskRow: vi.fn(() => null),
}))

// Mock virtual scrolling: return a virtual-item array matching count
vi.mock('@tanstack/react-virtual', () => ({
  useVirtualizer: vi.fn((options: { count: number }) => ({
    getVirtualItems: () =>
      options.count > 0
        ? Array.from({ length: options.count }, (_, i) => ({ index: i, start: i * 60, end: (i + 1) * 60 }))
        : [],
    getTotalSize: () => options.count * 60,
  })),
}))

import { fetchTasks, fetchSessions } from '@/api/endpoints'
import { useTaskStream } from '@/hooks/useTaskStream'
import { TaskRow } from '@/components/TaskRow'

const mockedFetchTasks = vi.mocked(fetchTasks)
const mockedFetchSessions = vi.mocked(fetchSessions)
const mockedUseTaskStream = vi.mocked(useTaskStream)

const mockTask = {
  id: 't1',
  session_id: 's1',
  title: '测试任务',
  status: 'done' as const,
  voice: 'v1',
  model: 'm1',
  style: null,
  total_chunks: 1,
  done_chunks: 1,
  failed_chunks: 0,
  duration_ms: 1000,
  error: null,
  has_audio: true,
  created_at: '2026-01-01T00:00:00Z',
  completed_at: '2026-01-01T00:00:01Z',
} as any

const mockTaskPage = {
  data: [mockTask],
  total: 1,
  page: 0,
  page_size: 100,
} as any

const mockSessionPage = {
  data: [{ id: 's1', name: '会话1', status: 'completed', total_tasks: 1, done_tasks: 1, failed_tasks: 0, created_at: '2026-01-01T00:00:00Z' }],
  total: 1,
  page: 0,
  page_size: 100,
} as any

beforeEach(() => {
  vi.clearAllMocks()
  mockedFetchSessions.mockResolvedValue(mockSessionPage)
  mockedUseTaskStream.mockReturnValue(undefined)
})

describe('TaskListPage', () => {
  it('渲染过滤栏（状态选择、会话选择、搜索框、搜索按钮）', () => {
    mockedFetchTasks.mockResolvedValue(mockTaskPage)
    renderWithRouter(<TaskListPage />)

    expect(screen.getByText('全部状态')).toBeInTheDocument()
    expect(screen.getByText('全部会话')).toBeInTheDocument()
    expect(screen.getByPlaceholderText(/搜索标题/)).toBeInTheDocument()
    expect(screen.getByText('搜索')).toBeInTheDocument()
  })

  it('渲染表头', () => {
    mockedFetchTasks.mockResolvedValue(mockTaskPage)
    renderWithRouter(<TaskListPage />)

    expect(screen.getByText('任务')).toBeInTheDocument()
    expect(screen.getByText('状态')).toBeInTheDocument()
    expect(screen.getByText('进度')).toBeInTheDocument()
    expect(screen.getByText('音色 · 模型')).toBeInTheDocument()
    expect(screen.getByText('时长')).toBeInTheDocument()
    expect(screen.getByText('创建时间')).toBeInTheDocument()
  })

  it('无任务时显示空状态', async () => {
    mockedFetchTasks.mockResolvedValue({ data: [], total: 0, page: 0, page_size: 100 } as any)
    renderWithRouter(<TaskListPage />)

    await screen.findByText('暂无任务')
  })

  it('有任务时渲染 TaskRow', async () => {
    mockedFetchTasks.mockResolvedValue(mockTaskPage)
    vi.mocked(TaskRow).mockReturnValue(<div data-testid="task-row">任务行</div>)

    renderWithRouter(<TaskListPage />)

    await screen.findByTestId('task-row')
  })

  it('加载时显示加载中', () => {
    mockedFetchTasks.mockImplementation(() => new Promise(() => {}))
    renderWithRouter(<TaskListPage />)

    expect(screen.getByText('加载中…')).toBeInTheDocument()
  })

  it('错误时显示错误通知', async () => {
    mockedFetchTasks.mockRejectedValue(new Error('网络错误'))
    renderWithRouter(<TaskListPage />)

    await screen.findByText('网络错误')
  })

  it('搜索按钮触发搜索', async () => {
    const user = userEvent.setup()
    mockedFetchTasks.mockResolvedValue(mockTaskPage)

    renderWithRouter(<TaskListPage />)

    // wait for the initial load
    await screen.findByText(/共 1 条/)

    mockedFetchTasks.mockClear()

    // type a search term
    await user.type(screen.getByPlaceholderText(/搜索标题/), '关键词')
    await user.click(screen.getByText('搜索'))

    // wait for the search request
    await waitFor(() => {
      expect(mockedFetchTasks).toHaveBeenCalledWith(
        expect.objectContaining({ search: '关键词' }),
      )
    })
  })

  it('状态过滤切换', async () => {
    const user = userEvent.setup()
    mockedFetchTasks.mockResolvedValue(mockTaskPage)

    renderWithRouter(<TaskListPage />)

    // wait for the initial load
    await screen.findByText(/共 1 条/)

    mockedFetchTasks.mockClear()

    // pick a status in the selector
    const statusSelect = screen.getByText('全部状态').closest('select')!
    await user.selectOptions(statusSelect, 'done')

    // wait for the request
    await waitFor(() => {
      expect(mockedFetchTasks).toHaveBeenCalledWith(
        expect.objectContaining({ status: 'done' }),
      )
    })
  })

  it('显示总数', async () => {
    mockedFetchTasks.mockResolvedValue(mockTaskPage)
    renderWithRouter(<TaskListPage />)

    await screen.findByText(/共 1 条/)
  })

  it('任务列表为空时显示空状态提示', async () => {
    mockedFetchTasks.mockResolvedValue({ data: [], total: 0, page: 0, page_size: 100 } as any)
    renderWithRouter(<TaskListPage />)

    await screen.findByText('暂无任务')
    expect(screen.getByText(/前往工作台/)).toBeInTheDocument()
  })
})
