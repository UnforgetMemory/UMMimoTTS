import { defineStore } from 'pinia'
import { ref, computed } from 'vue'

/** 检测是否是 Vercel Gateway 环境变量占位符（未被替换） */
function isVercelPlaceholder(key: string): boolean {
  return /^__VG_\w+__$/.test(key)
}

export const useConfigStore = defineStore('config', () => {
  const apiKey = ref<string>('')
  const selectedVoice = ref<string>('')
  const selectedModel = ref<string>('mimo-v2.5-tts')

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
  }
  // 初始化时从 localStorage 加载
  loadFromStorage()

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

  return {
    apiKey,
    selectedVoice,
    selectedModel,
    hasValidKey,
    autoClearPlaceholder,
    loadFromStorage,
    saveApiKey,
    clearApiKey,
    setVoice,
    setModel,
  }
})
