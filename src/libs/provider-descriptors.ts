import type { ApiType } from './types'

export interface ProviderDescriptor {
  value: ApiType
  label: string
  group: 'Hosted' | 'Local' | 'Compatible'
  allowsCustomBaseUrl: boolean
  requiresBaseUrl: boolean
  supportsModelListing: boolean
  requiresApiKey: boolean
}

const descriptorList: readonly ProviderDescriptor[] = [
  { value: 'open_ai', label: 'OpenAI', group: 'Hosted', allowsCustomBaseUrl: false, requiresBaseUrl: false, supportsModelListing: true, requiresApiKey: true },
  { value: 'deep_seek', label: 'DeepSeek', group: 'Hosted', allowsCustomBaseUrl: false, requiresBaseUrl: false, supportsModelListing: true, requiresApiKey: true },
  { value: 'anthropic', label: 'Anthropic', group: 'Hosted', allowsCustomBaseUrl: false, requiresBaseUrl: false, supportsModelListing: true, requiresApiKey: true },
  { value: 'azure', label: 'Azure OpenAI', group: 'Hosted', allowsCustomBaseUrl: true, requiresBaseUrl: true, supportsModelListing: false, requiresApiKey: true },
  { value: 'doubleword', label: 'Doubleword', group: 'Hosted', allowsCustomBaseUrl: false, requiresBaseUrl: false, supportsModelListing: false, requiresApiKey: true },
  { value: 'cohere', label: 'Cohere', group: 'Hosted', allowsCustomBaseUrl: false, requiresBaseUrl: false, supportsModelListing: false, requiresApiKey: true },
  { value: 'gemini', label: 'Google Gemini', group: 'Hosted', allowsCustomBaseUrl: false, requiresBaseUrl: false, supportsModelListing: true, requiresApiKey: true },
  { value: 'groq', label: 'Groq', group: 'Hosted', allowsCustomBaseUrl: false, requiresBaseUrl: false, supportsModelListing: false, requiresApiKey: true },
  { value: 'hugging_face', label: 'Hugging Face', group: 'Hosted', allowsCustomBaseUrl: false, requiresBaseUrl: false, supportsModelListing: false, requiresApiKey: true },
  { value: 'hyperbolic', label: 'Hyperbolic', group: 'Hosted', allowsCustomBaseUrl: false, requiresBaseUrl: false, supportsModelListing: false, requiresApiKey: true },
  { value: 'minimax', label: 'MiniMax', group: 'Hosted', allowsCustomBaseUrl: false, requiresBaseUrl: false, supportsModelListing: false, requiresApiKey: true },
  { value: 'mira', label: 'Mira', group: 'Hosted', allowsCustomBaseUrl: false, requiresBaseUrl: false, supportsModelListing: false, requiresApiKey: true },
  { value: 'mistral', label: 'Mistral', group: 'Hosted', allowsCustomBaseUrl: false, requiresBaseUrl: false, supportsModelListing: true, requiresApiKey: true },
  { value: 'moonshot', label: 'Moonshot', group: 'Hosted', allowsCustomBaseUrl: false, requiresBaseUrl: false, supportsModelListing: false, requiresApiKey: true },
  { value: 'open_router', label: 'OpenRouter', group: 'Hosted', allowsCustomBaseUrl: false, requiresBaseUrl: false, supportsModelListing: true, requiresApiKey: true },
  { value: 'perplexity', label: 'Perplexity', group: 'Hosted', allowsCustomBaseUrl: false, requiresBaseUrl: false, supportsModelListing: false, requiresApiKey: true },
  { value: 'together', label: 'Together AI', group: 'Hosted', allowsCustomBaseUrl: false, requiresBaseUrl: false, supportsModelListing: false, requiresApiKey: true },
  { value: 'x_ai', label: 'xAI', group: 'Hosted', allowsCustomBaseUrl: false, requiresBaseUrl: false, supportsModelListing: false, requiresApiKey: true },
  { value: 'xiaomi_mimo', label: 'Xiaomi MiMo', group: 'Hosted', allowsCustomBaseUrl: false, requiresBaseUrl: false, supportsModelListing: true, requiresApiKey: true },
  { value: 'z_ai', label: 'Z.ai', group: 'Hosted', allowsCustomBaseUrl: false, requiresBaseUrl: false, supportsModelListing: false, requiresApiKey: true },
  { value: 'ollama', label: 'Ollama', group: 'Local', allowsCustomBaseUrl: true, requiresBaseUrl: false, supportsModelListing: true, requiresApiKey: false },
  { value: 'llamafile', label: 'Llamafile', group: 'Local', allowsCustomBaseUrl: true, requiresBaseUrl: false, supportsModelListing: false, requiresApiKey: false },
  { value: 'open_ai_compatible', label: 'OpenAI Compatible', group: 'Compatible', allowsCustomBaseUrl: true, requiresBaseUrl: true, supportsModelListing: true, requiresApiKey: true },
]

export const providerDescriptors: readonly ProviderDescriptor[] = descriptorList

export const providerSelectOptions = (['Hosted', 'Local', 'Compatible'] as const).map((group) => ({
  type: 'group' as const,
  label: group,
  key: group,
  children: descriptorList
    .filter((descriptor) => descriptor.group === group)
    .map(({ label, value }) => ({ label, value })),
}))

export function providerDescriptor(value?: ApiType): ProviderDescriptor {
  return descriptorList.find((descriptor) => descriptor.value === value)
    ?? descriptorList.find((descriptor) => descriptor.value === 'open_ai_compatible')!
}
