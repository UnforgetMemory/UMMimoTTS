import type { DomainEvent } from '@/api/events'

/** Parse an SSE `data:` payload into a DomainEvent (`type`-tagged JSON);
 * malformed input → null. */
export function parseSseMessage(data: string): DomainEvent | null {
  try {
    const obj: unknown = JSON.parse(data)
    if (typeof obj !== 'object' || obj === null) return null
    if (typeof (obj as { type?: unknown }).type !== 'string') return null
    return obj as DomainEvent
  } catch {
    return null
  }
}
