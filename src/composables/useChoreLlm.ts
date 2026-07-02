import { ref, computed } from 'vue'
import { useProviderStore } from '../stores/provider'
import { mcpGetChoreLlm, mcpSetChoreLlm, type ChoreLlmRef } from '../libs/commands'

export function useChoreLlm() {
  const providerStore = useProviderStore()
  const choreLlm = ref<ChoreLlmRef | null>(null)
  const loading = ref(false)

  const providerOptions = computed(() =>
    providerStore.providers.map((p) => ({ label: p.display_name || p.name, value: p.name }))
  )

  const modelOptions = computed(() => {
    const providerName = choreLlm.value?.provider
    if (!providerName) return []
    const provider = providerStore.providers.find((p) => p.name === providerName)
    if (!provider) return []
    return provider.models
      .filter((m) => m.model_info.type === 'text_generation')
      .map((m) => ({ label: m.metadata.display_name || m.metadata.name, value: m.metadata.name }))
  })

  const load = async () => {
    loading.value = true
    try {
      choreLlm.value = await mcpGetChoreLlm()
    } finally {
      loading.value = false
    }
  }

  const save = async () => {
    await mcpSetChoreLlm(choreLlm.value)
  }

  const clear = async () => {
    choreLlm.value = null
    await mcpSetChoreLlm(null)
  }

  load()

  return { choreLlm, loading, providerOptions, modelOptions, save, clear }
}
