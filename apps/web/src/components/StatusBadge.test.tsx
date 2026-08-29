import { render, screen } from '@testing-library/react'
import { TaskStatusBadge, SessionStatusBadge, ChunkStatusBadge } from './StatusBadge'
import type { TaskStatus, SessionStatus, ChunkStatus } from '@/api/endpoints'

describe('TaskStatusBadge', () => {
  it.each([
    ['pending', '待处理'],
    ['queued', '排队中'],
    ['synthesizing', '合成中'],
    ['merging', '合并中'],
    ['done', '已完成'],
    ['failed', '失败'],
    ['cancelled', '已取消'],
  ])('status=%s 渲染标签"%s"', (status, label) => {
    render(<TaskStatusBadge status={status as TaskStatus} />)
    expect(screen.getByText(label)).toBeInTheDocument()
  })

  it('未知 status 回退到"未知"', () => {
    render(<TaskStatusBadge status={'unknown' as any} />)
    expect(screen.getByText('未知')).toBeInTheDocument()
  })

  it('每个 status 有对应的样式类', () => {
    const { container } = render(<TaskStatusBadge status="done" />)
    const badge = container.firstChild as HTMLElement
    expect(badge).toBeInTheDocument()
    expect(badge.className).toContain('bg-green-500/10')
    expect(badge.className).toContain('text-green-600')
  })

  it('渲染为 span 元素', () => {
    render(<TaskStatusBadge status="pending" />)
    const badge = screen.getByText('待处理')
    expect(badge.tagName).toBe('SPAN')
  })

  it('包含基础样式类', () => {
    render(<TaskStatusBadge status="pending" />)
    const badge = screen.getByText('待处理')
    expect(badge.className).toContain('inline-flex')
    expect(badge.className).toContain('rounded-md')
    expect(badge.className).toContain('text-xs')
    expect(badge.className).toContain('font-medium')
  })
})

describe('SessionStatusBadge', () => {
  it.each([
    ['active', '进行中'],
    ['completed', '已完成'],
    ['failed', '失败'],
    ['cancelled', '已取消'],
  ])('status=%s 渲染标签"%s"', (status, label) => {
    render(<SessionStatusBadge status={status as SessionStatus} />)
    expect(screen.getByText(label)).toBeInTheDocument()
  })

  it('未知 status 回退到"未知"', () => {
    render(<SessionStatusBadge status={'unknown' as any} />)
    expect(screen.getByText('未知')).toBeInTheDocument()
  })

  it('active 状态有蓝色样式', () => {
    const { container } = render(<SessionStatusBadge status="active" />)
    const badge = container.firstChild as HTMLElement
    expect(badge.className).toContain('bg-blue-500/10')
    expect(badge.className).toContain('text-blue-600')
  })
})

describe('ChunkStatusBadge', () => {
  it.each([
    ['pending', '待合成'],
    ['inflight', '合成中'],
    ['done', '已完成'],
    ['failed', '失败'],
  ])('status=%s 渲染标签"%s"', (status, label) => {
    render(<ChunkStatusBadge status={status as ChunkStatus} />)
    expect(screen.getByText(label)).toBeInTheDocument()
  })

  it('未知 status 回退到"未知"', () => {
    render(<ChunkStatusBadge status={'unknown' as any} />)
    expect(screen.getByText('未知')).toBeInTheDocument()
  })

  it('inflight 状态有品牌色样式', () => {
    const { container } = render(<ChunkStatusBadge status="inflight" />)
    const badge = container.firstChild as HTMLElement
    expect(badge.className).toContain('bg-brand-soft')
    expect(badge.className).toContain('text-brand')
  })
})

describe('Badge 通用行为', () => {
  it('渲染为 span 而非 button', () => {
    render(<TaskStatusBadge status="done" />)
    const badge = screen.getByText('已完成')
    expect(badge.tagName).not.toBe('BUTTON')
  })

  it('不渲染任何交互元素', () => {
    const { container } = render(<TaskStatusBadge status="pending" />)
    expect(container.querySelector('button')).toBeNull()
    expect(container.querySelector('input')).toBeNull()
  })
})
