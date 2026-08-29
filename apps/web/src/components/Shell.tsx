import { useEffect, useState } from 'react'
import { Link, NavLink, Outlet } from 'react-router'
import { useConfigStore } from '@/stores/config'
import { useStatsStore } from '@/stores/stats'
import { useEventSource } from '@/hooks/useEventSource'
import { ListIcon, SettingsIcon, UploadIcon, ZapIcon } from './Icons'
import { ThemeToggle } from './ThemeToggle'
import { ProviderHealthBar } from './ProviderHealthBar'

const NAV = [
  { to: '/', label: '工作台', icon: ZapIcon, end: true },
  { to: '/import', label: '批量导入', icon: UploadIcon, end: false },
  { to: '/tasks', label: '任务历史', icon: ListIcon, end: false },
  { to: '/settings', label: '设置', icon: SettingsIcon, end: false },
]

export function Shell() {
  const loadConfig = useConfigStore((s) => s.load)
  const announcement = useConfigStore((s) => s.config?.announcement)
  const refreshStats = useStatsStore((s) => s.refresh)
  const [showAuthBanner, setShowAuthBanner] = useState(false)

  useEffect(() => {
    void loadConfig()
  }, [loadConfig])

  // /stats poll (5s): top-bar queue depth + breaker countdown.
  useEffect(() => {
    void refreshStats()
    const t = setInterval(() => void refreshStats(), 5000)
    return () => clearInterval(t)
  }, [refreshStats])

  // Global 401 banner: missing/expired token → point the user at Settings.
  useEffect(() => {
    const onUnauthorized = () => setShowAuthBanner(true)
    window.addEventListener('um-mimotts:unauthorized', onUnauthorized)
    return () => window.removeEventListener('um-mimotts:unauthorized', onUnauthorized)
  }, [])

  // providers channel (token via query): provider_health → refresh stats now
  // so the countdown reacts immediately instead of waiting for the 5s poll.
  useEventSource({
    channel: 'providers',
    onEvent: (e) => {
      if (e.type === 'provider_health') void refreshStats()
    },
  })

  return (
    <div className="flex h-full">
      {/* narrow left nav */}
      <aside className="flex w-48 shrink-0 flex-col border-r border-border bg-surface-2">
        <div className="flex h-14 items-center gap-2 border-b border-border px-4">
          <span className="h-2.5 w-2.5 rounded-full bg-brand" />
          <span className="text-sm font-semibold text-ink">UM-MimoTTS</span>
        </div>
        <nav className="flex-1 space-y-1 p-2">
          {NAV.map((item) => (
            <NavLink
              key={item.to}
              to={item.to}
              end={item.end}
              className={({ isActive }) =>
                `flex items-center gap-2.5 rounded-lg px-3 py-2 text-sm transition-colors ${
                  isActive
                    ? 'bg-brand-soft font-medium text-brand'
                    : 'text-ink-secondary hover:bg-surface-3 hover:text-ink'
                }`
              }
            >
              <item.icon className="h-4 w-4" />
              {item.label}
            </NavLink>
          ))}
        </nav>
        <div className="num border-t border-border p-3 text-xs text-ink-tertiary">v{__APP_VERSION__}</div>
      </aside>

      {/* top bar + main area */}
      <div className="flex min-w-0 flex-1 flex-col">
        <header className="flex h-14 items-center gap-3 border-b border-border px-4">
          <h1 className="text-sm font-semibold text-ink">UM-MimoTTS 工作台</h1>
          <div className="num hidden flex-1 truncate text-xs text-ink-tertiary md:block">
            {announcement ? `📢 ${announcement}` : '公告条占位 — 暂无公告'}
          </div>
          <ProviderHealthBar />
          <ThemeToggle />
        </header>

        {showAuthBanner ? (
          <div className="flex items-center gap-2 border-b border-amber-500/30 bg-amber-500/10 px-4 py-1.5 text-xs text-amber-700 dark:text-amber-400">
            <span>未授权：请先填写 API Token。</span>
            <Link to="/settings" className="font-medium underline">
              前往设置
            </Link>
            <button
              type="button"
              className="ml-auto text-ink-tertiary hover:text-ink"
              onClick={() => setShowAuthBanner(false)}
            >
              关闭
            </button>
          </div>
        ) : null}

        <main className="scrollbar-thin flex-1 overflow-y-auto">
          <Outlet />
        </main>
      </div>
    </div>
  )
}
