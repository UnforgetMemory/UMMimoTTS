import { defineStore } from 'pinia'
import { ref } from 'vue'

export const useConfigStore = defineStore('config', () => {
  const apiKey = ref<string>('')
  const selectedVoice = ref<string>('')
  const selectedModel = ref<string>('mimo-v2.5-tts')

  // 从 localStorage 加载
  function loadFromStorage() {
    const storedApiKey = localStorage.getItem('mimo_api_key')
    const storedVoice = localStorage.getItem('mimo_selected_voice')
    const storedModel = localStorage.getItem('mimo_selected_model')
    
    if (storedApiKey) apiKey.value = storedApiKey
    if (storedVoice) selectedVoice.value = storedVoice
    if (storedModel) selectedModel.value = storedModel
  }

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
    loadFromStorage,
    saveApiKey,
    clearApiKey,
    setVoice,
    setModel,
  }
})
