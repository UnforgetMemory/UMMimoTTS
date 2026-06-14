import axios from 'axios'
import type { ApiError } from '@/types/api'

export function normalizeError(e: unknown): ApiError {
  if (axios.isAxiosError(e)) {
    return {
      code: (e.response?.data as any)?.code || 'UNKNOWN',
      message: (e.response?.data as any)?.error || e.message || '网络错误',
      retry: e.response?.status !== 400,
    }
  }
  return { code: 'UNKNOWN', message: '未知错误', retry: true }
}
