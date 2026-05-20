import { toast } from 'vue-sonner'

/**
 * 统一 API 错误处理
 * @param error 错误对象
 * @param defaultMessage 默认错误消息
 * @returns 错误消息
 */
export function handleApiError(error: any, defaultMessage: string = '操作失败'): string {
  console.error('API Error:', error)
  
  const message = error.response?.data?.message 
    || error.message 
    || defaultMessage
  
  toast.error(message)
  return message
}

/**
 * 网络错误处理
 */
export function handleNetworkError(): void {
  toast.error('网络连接失败，请检查网络设置')
}
