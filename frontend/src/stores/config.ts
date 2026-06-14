import { defineStore } from 'pinia'
import { ref, computed } from 'vue'
import { fetchConfig, apiV2, type VoicePreset, type ModelPreset, type ProviderInfo } from '@/api/client'

/** 检测是否是 Vercel Gateway 环境变量占位符（未被替换） */
function isVercelPlaceholder(key: string): boolean {
  return /^__VG_\w+__$/.test(key)
}

/** Local fallback voices used when backend is unreachable */
const FALLBACK_VOICES: VoicePreset[] = [
  { id: '冰糖', name: '冰糖', language: '中文', gender: '女性', style: '活泼少女', preview_url: 'https://aistudio-cdn.xiaomimimo.com/xiaomimimo-static/tts/audio/bingtang.wav' },
  { id: '茉莉', name: '茉莉', language: '中文', gender: '女性', style: '知性女声', preview_url: 'https://aistudio-cdn.xiaomimimo.com/xiaomimimo-static/tts/audio/moli.wav' },
  { id: '苏打', name: '苏打', language: '中文', gender: '男性', style: '阳光少年', preview_url: 'https://aistudio-cdn.xiaomimimo.com/xiaomimimo-static/tts/audio/suda.wav' },
  { id: '白桦', name: '白桦', language: '中文', gender: '男性', style: '成熟男声', preview_url: 'https://aistudio-cdn.xiaomimimo.com/xiaomimimo-static/tts/audio/baihua.wav' },
  { id: 'Mia', name: 'Mia', language: 'English', gender: 'Female', style: 'Lively girl', preview_url: 'https://aistudio-cdn.xiaomimimo.com/xiaomimimo-static/tts/audio/mia.wav' },
  { id: 'Chloe', name: 'Chloe', language: 'English', gender: 'Female', style: 'Sweet Dreamy', preview_url: 'https://aistudio-cdn.xiaomimimo.com/xiaomimimo-static/tts/audio/chloe.wav' },
  { id: 'Milo', name: 'Milo', language: 'English', gender: 'Male', style: 'Sunny boy', preview_url: 'https://aistudio-cdn.xiaomimimo.com/xiaomimimo-static/tts/audio/milo.wav' },
  { id: 'Dean', name: 'Dean', language: 'English', gender: 'Male', style: 'Steady Gentle', preview_url: 'https://aistudio-cdn.xiaomimimo.com/xiaomimimo-static/tts/audio/dean.wav' },
]

export const useConfigStore = defineStore('config', () => {
  const apiKey = ref<string>('')
  const selectedVoice = ref<string>('')
  const selectedModel = ref<string>('mimo-v2.5-tts')

  // Config-loaded state
  const voices = ref<VoicePreset[]>([])
  const models = ref<ModelPreset[]>([])
  const providers = ref<ProviderInfo[]>([])
  const selectedProviderId = ref<string>('')
  const configLoaded = ref(false)

  /** API Key 是否有效（非空且非占位符） */
  const hasValidKey = computed(() => {
    return apiKey.value.length > 0 && !isVercelPlaceholder(apiKey.value)
  })

  /** 自动清理已检测到的占位符 Key */
  function autoClearPlaceholder() {
    if (apiKey.value && isVercelPlaceholder(apiKey.value)) {
      clearApiKey()
    }
  }

  // 从 localStorage 加载
  function loadFromStorage() {
    const storedApiKey = localStorage.getItem('mimo_api_key')
    const storedVoice = localStorage.getItem('mimo_selected_voice')
    const storedModel = localStorage.getItem('mimo_selected_model')
    const storedProvider = localStorage.getItem('mimo_selected_provider')
    
    if (storedApiKey) {
      // 检测到 Vercel 占位符 → 自动清除
      if (isVercelPlaceholder(storedApiKey)) {
        clearApiKey()
      } else {
        apiKey.value = storedApiKey
      }
    }
    if (storedVoice) selectedVoice.value = storedVoice
    if (storedModel) selectedModel.value = storedModel
    if (storedProvider) selectedProviderId.value = storedProvider
  }
  // 初始化时从 localStorage 加载
  loadFromStorage()

  // Load voice/model presets from backend, with local fallback
  async function loadConfig() {
    try {
      const config = await fetchConfig()
      voices.value = config.voices
      models.value = config.models
    } catch {
      // Fallback to local presets when backend is unreachable
      voices.value = FALLBACK_VOICES
      models.value = [{ id: 'mimo-v2.5-tts', name: 'mimo-v2.5-tts', description: '小米 MIMO TTS 模型，支持预置音色' }]
    }
    configLoaded.value = true
  }

  // Load providers from backend
  async function loadProviders() {
    try {
      providers.value = await apiV2.listProviders()
      // If no provider selected yet, default to the backend's default provider
      if (!selectedProviderId.value) {
        const def = providers.value.find(p => p.is_default)
        if (def) selectedProviderId.value = def.id
      }
    } catch {
      // Silently fail — providers are optional; tasks work without them
    }
  }

  /** Check if a voice code is valid (exists in loaded presets) */
  function isValidVoice(code: string): boolean {
    return voices.value.some(v => v.id === code)
  }

  /** Check if a model code is valid (exists in loaded presets) */
  function isValidModel(code: string): boolean {
    return models.value.some(m => m.id === code)
  }

  // Load config on store initialization (non-blocking)
  loadConfig()
  loadProviders()

  // 保存到 localStorage
  function saveApiKey(key: string) {
    apiKey.value = key
    localStorage.setItem('mimo_api_key', key)
  }

  function clearApiKey() {
    apiKey.value = ''
    localStorage.removeItem('mimo_api_key')
  }

  function setVoice(voice: string) {
    selectedVoice.value = voice
    localStorage.setItem('mimo_selected_voice', voice)
  }

  function setModel(model: string) {
    selectedModel.value = model
    localStorage.setItem('mimo_selected_model', model)
  }

  function setProvider(id: string) {
    selectedProviderId.value = id
    localStorage.setItem('mimo_selected_provider', id)
  }

  /** The default provider as marked by the backend, or the first configured one */
  const defaultProvider = computed(() => {
    return providers.value.find(p => p.is_default) || providers.value.find(p => p.is_configured) || null
  })

  /** Providers that have an API key configured */
  const configuredProviders = computed(() => providers.value.filter(p => p.is_configured))

  /** Whether any provider has is_configured=true */
  const hasConfiguredProvider = computed(() => providers.value.some(p => p.is_configured))

  /** ID of the first configured provider */
  const configuredProviderId = computed(() => {
    const first = providers.value.find(p => p.is_configured)
    return first ? first.id : null
  })

  return {
    apiKey,
    selectedVoice,
    selectedModel,
    selectedProviderId,
    hasValidKey,
    autoClearPlaceholder,
    loadFromStorage,
    saveApiKey,
    clearApiKey,
    setVoice,
    setModel,
    setProvider,
    voices,
    models,
    providers,
    configLoaded,
    loadConfig,
    loadProviders,
    isValidVoice,
    isValidModel,
    defaultProvider,
    configuredProviders,
    hasConfiguredProvider,
    configuredProviderId,
  }
})
