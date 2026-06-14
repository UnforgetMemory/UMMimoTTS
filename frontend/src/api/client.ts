import axios from 'axios'
import { toast } from 'vue-sonner'
import { normalizeError } from '@/utils/error'

const apiClient = axios.create({
  baseURL: '',
  timeout: 120000,
  headers: { 'Content-Type': 'application/json' },
})

apiClient.interceptors.response.use(
  (response) => response,
  (error) => {
    const err = normalizeError(error)
    if (err.code !== 'UNKNOWN' || error.response?.status !== 404) {
      toast.error(err.message)
    }
    return Promise.reject(err)
  }
)

export default apiClient
