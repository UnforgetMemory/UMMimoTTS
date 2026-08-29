import { describe, it, expect } from 'vitest'
import { nextBackoffMs, BACKOFF_MAX_MS, BACKOFF_BASE_MS } from './backoff'

describe('nextBackoffMs', () => {
  it('指数增长', () => {
    const one = () => 1 // upper bound for easy assertions
    expect(nextBackoffMs(0, one)).toBe(BACKOFF_BASE_MS)
    expect(nextBackoffMs(1, one)).toBe(2000)
    expect(nextBackoffMs(2, one)).toBe(4000)
  })

  it('封顶 30s', () => {
    const one = () => 1
    expect(nextBackoffMs(5, one)).toBe(BACKOFF_MAX_MS)
    expect(nextBackoffMs(10, one)).toBe(BACKOFF_MAX_MS)
    expect(nextBackoffMs(100, one)).toBe(BACKOFF_MAX_MS)
  })

  it('full jitter：random=0 时回落 1ms 地板', () => {
    expect(nextBackoffMs(3, () => 0)).toBe(1)
  })

  it('负 attempt 按 0 处理', () => {
    expect(nextBackoffMs(-5, () => 1)).toBe(BACKOFF_BASE_MS)
  })
})
