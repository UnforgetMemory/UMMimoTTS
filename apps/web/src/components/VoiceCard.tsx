import type { VoicePreset } from '@/api/endpoints'
import { PauseIcon, PlayIcon } from './Icons'

interface VoiceCardProps {
  voice: VoicePreset
  selected: boolean
  onSelect: (id: string) => void
  playing: boolean
  /** Preview always goes through /voices/{id}/preview (backend 302 → CDN
   * allowlist). */
  onTogglePlay: (id: string) => void
}

export function VoiceCard({ voice, selected, onSelect, playing, onTogglePlay }: VoiceCardProps) {
  return (
    <div
      role="button"
      tabIndex={0}
      onClick={() => onSelect(voice.id)}
      onKeyDown={(e) => {
        if (e.key === 'Enter' || e.key === ' ') {
          e.preventDefault()
          onSelect(voice.id)
        }
      }}
      className={`group relative flex cursor-pointer flex-col gap-1 rounded-xl border p-3 transition-colors ${
        selected
          ? 'border-brand bg-brand-soft ring-2 ring-brand-ring'
          : 'border-border bg-surface-2 hover:border-brand/50 hover:bg-surface-3'
      }`}
    >
      <div className="flex items-center justify-between gap-2">
        <span className="truncate text-sm font-semibold text-ink">{voice.name}</span>
        {/* Preview always goes through the scoped proxy (/voices/{id}/preview) —
            gating on the upstream CDN field would silently hide the button. */}
        <button
          type="button"
          onClick={(e) => {
            e.stopPropagation()
            onTogglePlay(voice.id)
          }}
          onKeyDown={(e) => {
            // Keep Enter/Space inside the button: the card's own keyboard
            // handler would otherwise also fire (select instead of preview).
            e.stopPropagation()
          }}
          className="inline-flex h-7 w-7 shrink-0 items-center justify-center rounded-full bg-brand text-white transition-colors hover:bg-brand-hover"
          aria-label={playing ? '暂停试听' : '试听'}
          title={playing ? '暂停试听' : '试听'}
        >
          {playing ? <PauseIcon className="h-3.5 w-3.5" /> : <PlayIcon className="h-3.5 w-3.5" />}
        </button>
      </div>
      <div className="text-xs text-ink-tertiary">
        {voice.language} · {voice.gender}
      </div>
      {voice.style ? <div className="truncate text-xs text-ink-secondary">{voice.style}</div> : null}
    </div>
  )
}
