import apiClient from './client'
import type { VoicePreset, ModelPreset, ProviderInfo } from '@/types/config'

export const configApi = {
  async getConfig() {
    const { data } = await apiClient.get('/api/v2/config')
    return data as { voices: VoicePreset[]; models: ModelPreset[]; providers: ProviderInfo[] }
  },

  async listProviders() {
    const { data } = await apiClient.get('/api/v2/providers')
    return data as ProviderInfo[]
  },

  async updateProviderKey(id: string, apiKey: string) {
    await apiClient.put(`/api/v2/providers/${id}`, { api_key: apiKey })
  },

  async setDefaultProvider(id: string) {
    await apiClient.put(`/api/v2/providers/${id}/default`)
  },
}
