import { describe, it, expect } from 'vitest'
import {
  TASK_STATUS_LABELS,
  SESSION_STATUS_LABELS,
  CHUNK_STATUS_LABELS,
  isTerminalTaskStatus,
  taskProgress,
  formatDuration,
  formatCountdown,
  formatDateTime,
  extractTitle,
} from './status'

describe('状态中文标签', () => {
  it('任务状态映射完整', () => {
    expect(TASK_STATUS_LABELS.pending).toBe('待处理')
    expect(TASK_STATUS_LABELS.queued).toBe('排队中')
    expect(TASK_STATUS_LABELS.synthesizing).toBe('合成中')
    expect(TASK_STATUS_LABELS.merging).toBe('合并中')
    expect(TASK_STATUS_LABELS.done).toBe('已完成')
    expect(TASK_STATUS_LABELS.failed).toBe('失败')
    expect(TASK_STATUS_LABELS.cancelled).toBe('已取消')
  })

  it('会话状态映射完整', () => {
    expect(SESSION_STATUS_LABELS.active).toBe('进行中')
    expect(SESSION_STATUS_LABELS.completed).toBe('已完成')
  })

  it('分片状态映射完整', () => {
    expect(CHUNK_STATUS_LABELS.pending).toBe('待合成')
    expect(CHUNK_STATUS_LABELS.inflight).toBe('合成中')
    expect(CHUNK_STATUS_LABELS.done).toBe('已完成')
    expect(CHUNK_STATUS_LABELS.failed).toBe('失败')
  })
})

describe('isTerminalTaskStatus', () => {
  it('终态判定', () => {
    expect(isTerminalTaskStatus('done')).toBe(true)
    expect(isTerminalTaskStatus('failed')).toBe(true)
    expect(isTerminalTaskStatus('cancelled')).toBe(true)
    expect(isTerminalTaskStatus('pending')).toBe(false)
    expect(isTerminalTaskStatus('synthesizing')).toBe(false)
  })
})

describe('taskProgress', () => {
  it('钳制到 [0,1] 且 total<=0 归零', () => {
    expect(taskProgress(0, 0)).toBe(0)
    expect(taskProgress(2, 4)).toBe(0.5)
    expect(taskProgress(5, 4)).toBe(1)
    expect(taskProgress(-1, 4)).toBe(0)
  })
})

describe('formatDuration', () => {
  it('毫秒/秒/分', () => {
    expect(formatDuration(null)).toBe('—')
    expect(formatDuration(undefined)).toBe('—')
    expect(formatDuration(500)).toBe('500 毫秒')
    expect(formatDuration(45000)).toBe('45.0 秒')
    expect(formatDuration(83000)).toBe('1 分 23 秒')
    expect(formatDuration(60000)).toBe('1 分钟')
    expect(formatDuration(3_600_000)).toBe('1 小时')
  })

  it('边界进位：59950ms 显示为 1 分钟而非 60.0 秒', () => {
    expect(formatDuration(59950)).toBe('1 分钟')
    expect(formatDuration(59499)).toBe('59.5 秒')
  })
})

describe('formatCountdown', () => {
  it('倒计时', () => {
    expect(formatCountdown(0)).toBe('0 秒')
    expect(formatCountdown(45)).toBe('45 秒')
    expect(formatCountdown(90)).toBe('1 分 30 秒')
  })

  it('进位：整分钟/整小时', () => {
    expect(formatCountdown(60)).toBe('1 分钟')
    expect(formatCountdown(3600)).toBe('1 小时')
    expect(formatCountdown(3661)).toBe('1 小时 1 分')
  })
})

describe('formatDateTime', () => {
  it('非法输入返回占位', () => {
    expect(formatDateTime(null)).toBe('—')
    expect(formatDateTime('not-a-date')).toBe('—')
  })

  it('格式化合法时间', () => {
    expect(formatDateTime('2026-08-28T10:00:00Z')).toContain('2026-08-28')
  })
})

describe('extractTitle', () => {
  it('取首个非空行并去掉唱歌标签', () => {
    expect(extractTitle('你好，世界。\n第二行')).toBe('你好，世界。')
    expect(extractTitle('(唱歌)两只老虎跑得快\n')).toBe('两只老虎跑得快')
    expect(extractTitle('   ')).toBe('未命名任务')
  })

  it('超长截断', () => {
    expect(extractTitle('字'.repeat(100))).toHaveLength(61) // 60 chars + ellipsis
  })
})
