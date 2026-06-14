export interface VoicePreset {
  id: string
  name: string
  language: string
  gender: string
  style: string
  preview_url?: string
}

export interface ModelPreset {
  id: string
  name: string
  description?: string
}

export interface ProviderInfo {
  id: string
  name: string
  base_url: string
  is_configured: boolean
  is_default: boolean
}
