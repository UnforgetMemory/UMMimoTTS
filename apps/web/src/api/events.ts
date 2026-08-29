// SSE DomainEvent structures — source of truth is crates/mimotts-core/src/events.rs.
// The OpenAPI contract declares no schema for /events (text/event-stream only),
// so the backend bus `type`-tagged union (serde tag="type", snake_case) is
// mirrored here and kept apart from the REST types (v3.d.ts).

export type ProviderHealthState = 'degraded' | 'open' | 'half_open' | 'closed'

export type DomainEvent =
  | { type: 'task_status_changed'; task_id: string; session_id: string | null; status: string }
  | { type: 'chunk_completed'; chunk_id: string; task_id: string; seq: number; audio_path: string; duration_ms: number }
  | { type: 'chunk_failed'; chunk_id: string; task_id: string; seq: number; error: string }
  | { type: 'all_chunks_done'; task_id: string }
  | { type: 'task_completed'; task_id: string; session_id: string | null; output_path: string; duration_ms: number }
  | { type: 'task_failed'; task_id: string; session_id: string | null; error: string }
  | { type: 'session_updated'; session_id: string }
  | {
      type: 'provider_health'
      provider_id: string
      state: ProviderHealthState
      retry_after_secs: number | null
    }
