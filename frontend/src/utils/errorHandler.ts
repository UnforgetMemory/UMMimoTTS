import { toast } from 'vue-sonner'

/**
 * 统一 API 错误处理
 * @param error 错误对象
 * @param defaultMessage 默认错误消息
 * @returns 错误消息
 */
export function handleApiError(error: any, defaultMessage: string = '操作失败'): string {
  console.error('API Error:', error)
  
  const data = error.response?.data
  const code = data?.code
  const message = data?.error 
    || error.message 
    || defaultMessage
  
  const description = code ? `错误码: ${code}` : undefined
  toast.error(message, description ? { description } : undefined)
  return message
}

/**
 * 网络错误处理
 */
export function handleNetworkError(): void {
  toast.error('网络连接失败，请检查网络设置')
}
