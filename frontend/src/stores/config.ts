import { defineStore } from 'pinia'
import { ref, computed } from 'vue'
import { configApi } from '@/api/config'
import type { VoicePreset, ModelPreset, ProviderInfo } from '@/types/config'

const FALLBACK_VOICES: VoicePreset[] = [
  { id: '冰糖', name: '冰糖', language: '中文', gender: '女性', style: '活泼少女' },
  { id: '茉莉', name: '茉莉', language: '中文', gender: '女性', style: '知性女声' },
  { id: '苏打', name: '苏打', language: '中文', gender: '男性', style: '阳光少年' },
  { id: '白桦', name: '白桦', language: '中文', gender: '男性', style: '成熟男声' },
  { id: 'Mia', name: 'Mia', language: 'English', gender: 'Female', style: 'Lively girl' },
  { id: 'Chloe', name: 'Chloe', language: 'English', gender: 'Female', style: 'Sweet Dreamy' },
  { id: 'Milo', name: 'Milo', language: 'English', gender: 'Male', style: 'Sunny boy' },
  { id: 'Dean', name: 'Dean', language: 'English', gender: 'Male', style: 'Steady Gentle' },
]

export const useConfigStore = defineStore('config', () => {
  const apiKey = ref('')
  const selectedVoice = ref('')
  const selectedModel = ref('mimo-v2.5-tts')
  const voices = ref<VoicePreset[]>([])
  const models = ref<ModelPreset[]>([])
  const providers = ref<ProviderInfo[]>([])
  const selectedProviderId = ref('')
  const configLoaded = ref(false)

  const hasValidKey = computed(() => apiKey.value.length > 0)
  const hasConfiguredProvider = computed(() => providers.value.some((p: ProviderInfo) => p.is_configured))
  const defaultProvider = computed(() => providers.value.find((p: ProviderInfo) => p.is_default) || providers.value.find((p: ProviderInfo) => p.is_configured) || null)

  function loadFromStorage() {
    const k = localStorage.getItem('mimo_api_key')
    if (k) apiKey.value = k
    const v = localStorage.getItem('mimo_selected_voice')
    if (v) selectedVoice.value = v
    const m = localStorage.getItem('mimo_selected_model')
    if (m) selectedModel.value = m
    const p = localStorage.getItem('mimo_selected_provider')
    if (p) selectedProviderId.value = p
  }

  async function loadConfig() {
    try {
      const res = await configApi.getConfig()
      voices.value = res.voices.length > 0 ? res.voices : FALLBACK_VOICES
      models.value = res.models.length > 0 ? res.models : [{ id: 'mimo-v2.5-tts', name: 'mimo-v2.5-tts' }]
      providers.value = res.providers || []
    } catch {
      voices.value = FALLBACK_VOICES
      models.value = [{ id: 'mimo-v2.5-tts', name: 'mimo-v2.5-tts' }]
    }
    configLoaded.value = true
  }

  async function loadProviders() {
    try {
      const list = await configApi.listProviders()
      if (list.length > 0) providers.value = list
      if (!selectedProviderId.value) {
        const def = providers.value.find((p: ProviderInfo) => p.is_default)
        if (def) selectedProviderId.value = def.id
      }
    } catch (e: any) {
      console.error('Failed to load providers:', e.message)
    }
  }

  function saveApiKey(key: string) { apiKey.value = key; localStorage.setItem('mimo_api_key', key) }
  function clearApiKey() { apiKey.value = ''; localStorage.removeItem('mimo_api_key') }
  function setVoice(v: string) { selectedVoice.value = v; localStorage.setItem('mimo_selected_voice', v) }
  function setModel(m: string) { selectedModel.value = m; localStorage.setItem('mimo_selected_model', m) }
  function setProvider(id: string) { selectedProviderId.value = id; localStorage.setItem('mimo_selected_provider', id) }

  loadFromStorage()
  loadConfig()
  loadProviders()

  return { apiKey, selectedVoice, selectedModel, voices, models, providers, selectedProviderId, configLoaded, hasValidKey, hasConfiguredProvider, defaultProvider, loadFromStorage, loadConfig, loadProviders, saveApiKey, clearApiKey, setVoice, setModel, setProvider }
})
