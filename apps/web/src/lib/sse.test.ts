import { describe, it, expect } from 'vitest'
import { parseSseMessage } from './sse'

describe('parseSseMessage', () => {
  it('解析合法 type 标签事件', () => {
    const ev = parseSseMessage(
      '{"type":"task_status_changed","task_id":"t1","session_id":null,"status":"queued"}',
    )
    expect(ev).toEqual({ type: 'task_status_changed', task_id: 't1', session_id: null, status: 'queued' })
  })

  it('解析 provider_health 事件', () => {
    const ev = parseSseMessage(
      '{"type":"provider_health","provider_id":"p1","state":"open","retry_after_secs":42}',
    )
    expect(ev).toMatchObject({ type: 'provider_health', provider_id: 'p1', state: 'open', retry_after_secs: 42 })
  })

  it('非法 JSON → null', () => {
    expect(parseSseMessage('not-json')).toBeNull()
  })

  it('缺少 type 标签 → null', () => {
    expect(parseSseMessage('{"foo":1}')).toBeNull()
  })

  it('非对象 → null', () => {
    expect(parseSseMessage('"str"')).toBeNull()
    expect(parseSseMessage('123')).toBeNull()
  })
})
