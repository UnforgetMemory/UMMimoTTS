import { Spinner } from './ui'

export function PageLoader() {
  return (
    <div className="flex h-full min-h-[40vh] items-center justify-center text-ink-tertiary">
      <Spinner className="h-6 w-6" />
    </div>
  )
}
