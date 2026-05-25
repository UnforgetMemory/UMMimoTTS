import { ref, computed, onScopeDispose, getCurrentScope, type ComputedRef } from 'vue'

// ── Global single setInterval(1000) ──────────────────────────────

let globalTick = ref(0)
let intervalId: ReturnType<typeof setInterval> | null = null
let consumerCount = 0

function startGlobalTimer() {
  if (intervalId !== null) return
  intervalId = setInterval(() => {
    globalTick.value++
  }, 1000)
}

function stopGlobalTimerIfIdle() {
  if (consumerCount <= 0 && intervalId !== null) {
    clearInterval(intervalId)
    intervalId = null
  }
}

// ── Map-based task tracking ──────────────────────────────────────

interface TrackedTask {
  createdAt: number       // timestamp ms
  completedAt: number | null  // timestamp ms or null if still running
  startedAt: number       // when tracking started (for display after completion)
}

const trackedTasks = new Map<string, TrackedTask>()
const trackedReactions = new Map<string, Set<() => void>>()

/**
 * Start tracking elapsed time for a task.
 * Call this in onMounted of components that display a task's elapsed time.
 */
export function startTracking(taskId: string, createdAt: string, completedAt?: string | null) {
  consumerCount++
  startGlobalTimer()

  trackedTasks.set(taskId, {
    createdAt: new Date(createdAt).getTime(),
    completedAt: completedAt ? new Date(completedAt).getTime() : null,
    startedAt: Date.now(),
  })
}

/**
 * Stop tracking a task. Call this in onUnmounted.
 */
export function stopTracking(taskId: string) {
  trackedTasks.delete(taskId)
  trackedReactions.delete(taskId)
  consumerCount--
  stopGlobalTimerIfIdle()
}

/**
 * Get a reactive computed that returns elapsed seconds for a tracked task.
 * Falls back to completedAt - createdAt if task is done.
 */
export function getElapsed(taskId: string): ComputedRef<number> {
  // Register a reaction so the computed re-evaluates on tick
  if (!trackedReactions.has(taskId)) {
    trackedReactions.set(taskId, new Set())
  }

  const elapsed = computed(() => {
    // Touch globalTick to make this reactive to the global timer
    void globalTick.value

    const tracked = trackedTasks.get(taskId)
    if (!tracked) return 0

    const now = Date.now()
    if (tracked.completedAt) {
      // For completed tasks, return the duration at completion
      return Math.max(0, Math.floor((tracked.completedAt - tracked.createdAt) / 1000))
    }
    return Math.max(0, Math.floor((now - tracked.createdAt) / 1000))
  })

  // Register self-cleanup if in a component scope
  if (getCurrentScope()) {
    onScopeDispose(() => {
      trackedReactions.delete(taskId)
    })
  }

  return elapsed
}

// ── Legacy single-task helper (used by TaskItem) ────────────────

/**
 * Reactive elapsed time from a given `created_at` timestamp.
 * Uses the single global setInterval that ticks every second.
 * Automatically cleans up when the consumer disconnects.
 */
export function useElapsedTime(createdAt: string | (() => string)): ComputedRef<number> {
  consumerCount++
  startGlobalTimer()

  const createdRef = typeof createdAt === 'function' ? createdAt : () => createdAt

  const elapsed = computed(() => {
    // Access globalTick to make this reactive to the global timer
    void globalTick.value
    const start = new Date(createdRef()).getTime()
    if (isNaN(start)) return 0
    return Math.max(0, Math.floor((Date.now() - start) / 1000))
  })

  if (getCurrentScope()) {
    onScopeDispose(() => {
      consumerCount--
      stopGlobalTimerIfIdle()
    })
  }

  return elapsed
}
