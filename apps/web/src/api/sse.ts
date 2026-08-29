// SSE subscription URL builder: the channel authenticates via a scoped token
// (events:{channel}).
import { getScopedToken } from './scoped'

/**
 * Build the SSE subscription URL (async): `/api/v3/events?channel=...&token=<scoped>`.
 * EventSource cannot carry an Authorization header, so a short-lived scoped
 * credential rides the query instead.
 */
export async function buildEventUrl(channel: string): Promise<string> {
  const token = await getScopedToken(`events:${channel}`)
  return `/api/v3/events?channel=${encodeURIComponent(channel)}&token=${encodeURIComponent(token)}`
}
