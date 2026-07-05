import { defineStore } from 'pinia'
import { ref } from 'vue'
import type { PipelineConfig, ConversationLoopConfig } from '../libs/types'
import {
  settingsGetPipelineConfig,
  settingsUpdatePipelineConfig,
  settingsGetConversationConfig,
  settingsUpdateConversationConfig,
} from '../libs/commands'

export const useSettingsStore = defineStore('settings', () => {
  const pipelineConfig = ref<PipelineConfig | null>(null)
  const conversationConfig = ref<ConversationLoopConfig | null>(null)
  const isLoading = ref(false)

  const loadPipelineConfig = async () => {
    try {
      pipelineConfig.value = await settingsGetPipelineConfig()
    } catch (e) {
      console.error('Failed to load pipeline config:', e)
    }
  }

  const savePipelineConfig = async (config: PipelineConfig) => {
    isLoading.value = true
    try {
      await settingsUpdatePipelineConfig(config)
      pipelineConfig.value = config
    } finally {
      isLoading.value = false
    }
  }

  const loadConversationConfig = async () => {
    try {
      conversationConfig.value = await settingsGetConversationConfig()
    } catch (e) {
      console.error('Failed to load conversation config:', e)
    }
  }

  const saveConversationConfig = async (config: ConversationLoopConfig) => {
    isLoading.value = true
    try {
      await settingsUpdateConversationConfig(config)
      conversationConfig.value = config
    } finally {
      isLoading.value = false
    }
  }

  const init = async () => {
    await Promise.all([loadPipelineConfig(), loadConversationConfig()])
  }

  return {
    pipelineConfig,
    conversationConfig,
    isLoading,
    init,
    loadPipelineConfig,
    savePipelineConfig,
    loadConversationConfig,
    saveConversationConfig,
  }
})
