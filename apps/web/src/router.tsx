import { lazy, Suspense } from 'react'
import type { ReactNode } from 'react'
import { Link, createBrowserRouter, useRouteError } from 'react-router'
import { Shell } from '@/components/Shell'
import { PageLoader } from '@/components/PageLoader'

// Route-level code splitting keeps the main bundle small.
const Workbench = lazy(() => import('@/pages/Workbench'))
const ImportPage = lazy(() => import('@/pages/ImportPage'))
const TaskListPage = lazy(() => import('@/pages/TaskListPage'))
const TaskDetailPage = lazy(() => import('@/pages/TaskDetailPage'))
const SettingsPage = lazy(() => import('@/pages/SettingsPage'))

function withSuspense(node: ReactNode) {
  return <Suspense fallback={<PageLoader />}>{node}</Suspense>
}

function RouteError() {
  const error = useRouteError()
  const message = error instanceof Error ? error.message : String(error)
  return (
    <div className="flex h-full flex-col items-center justify-center gap-3 p-6 text-center">
      <div className="text-base font-semibold text-ink">页面加载出错</div>
      <div className="max-w-xl truncate text-sm text-ink-secondary">{message}</div>
      <Link to="/" className="text-sm font-medium text-brand hover:text-brand-hover">
        返回工作台
      </Link>
    </div>
  )
}

function NotFound() {
  return (
    <div className="flex h-full flex-col items-center justify-center gap-3 p-6 text-center">
      <div className="text-lg font-semibold text-ink">404</div>
      <div className="text-sm text-ink-secondary">页面不存在</div>
      <Link to="/" className="text-sm font-medium text-brand hover:text-brand-hover">
        返回工作台
      </Link>
    </div>
  )
}

export const router = createBrowserRouter([
  {
    path: '/',
    element: <Shell />,
    errorElement: <RouteError />,
    children: [
      { index: true, element: withSuspense(<Workbench />) },
      { path: 'import', element: withSuspense(<ImportPage />) },
      { path: 'tasks', element: withSuspense(<TaskListPage />) },
      { path: 'tasks/:id', element: withSuspense(<TaskDetailPage />) },
      { path: 'settings', element: withSuspense(<SettingsPage />) },
      { path: '*', element: <NotFound /> },
    ],
  },
])
