import { describe, it, expect } from 'vitest'
import type { ServerStats } from '@/api/endpoints'
import { anyOpenProvider, queueDepth, remainingOpenSecs, workerCount } from './stats'

const stats = (providers: ServerStats['providers'], queue_depth = 3, workers = 4): ServerStats => ({
  queue_depth,
  workers,
  providers,
})

describe('queueDepth / workerCount', () => {
  it('缺省回退 0', () => {
    expect(queueDepth(null)).toBe(0)
    expect(workerCount(null)).toBe(0)
    expect(queueDepth(stats(undefined))).toBe(3)
    expect(workerCount(stats(undefined))).toBe(4)
  })
})

describe('anyOpenProvider', () => {
  it('检测 open 供应商', () => {
    expect(anyOpenProvider(null)).toBe(false)
    expect(
      anyOpenProvider(stats([{ provider_id: 'a', open: true, retry_after_secs: 10 }])),
    ).toBe(true)
    expect(
      anyOpenProvider(stats([{ provider_id: 'a', open: false }])),
    ).toBe(false)
  })
})

describe('remainingOpenSecs', () => {
  it('随时间递减并钳制到 0', () => {
    const s = stats([
      { provider_id: 'a', open: true, retry_after_secs: 30 },
      { provider_id: 'b', open: true, retry_after_secs: 10 },
    ])
    expect(remainingOpenSecs(s, 0, 0)).toBe(30) // longest cooldown wins
    expect(remainingOpenSecs(s, 0, 15_000)).toBe(15)
    expect(remainingOpenSecs(s, 0, 60_000)).toBe(0)
  })

  it('无 open 供应商返回 null', () => {
    expect(remainingOpenSecs(stats([{ provider_id: 'a', open: false }]), 0, 0)).toBeNull()
    expect(remainingOpenSecs(null, 0, 0)).toBeNull()
  })
})
