import { describe, expect, it } from 'vitest'
import {
  providerDescriptor,
  providerDescriptors,
} from '../../libs/provider-descriptors'

describe('provider descriptors', () => {
  it('contains unique values for all native chat provider options', () => {
    const values = providerDescriptors.map((descriptor) => descriptor.value)
    expect(new Set(values).size).toBe(values.length)
    expect(values).toContain('deep_seek')
    expect(values).toContain('anthropic')
    expect(values).toContain('open_ai_compatible')
  })

  it('requires Base URL only for OpenAI Compatible', () => {
    expect(providerDescriptor('open_ai_compatible').requiresBaseUrl).toBe(true)
    expect(providerDescriptor('open_ai').requiresBaseUrl).toBe(false)
    expect(providerDescriptor('deep_seek').allowsCustomBaseUrl).toBe(false)
    expect(providerDescriptor('ollama').allowsCustomBaseUrl).toBe(true)
  })

  it('matches native model listing capability', () => {
    expect(providerDescriptor('deep_seek').supportsModelListing).toBe(true)
    expect(providerDescriptor('mistral').supportsModelListing).toBe(true)
    expect(providerDescriptor('groq').supportsModelListing).toBe(false)
  })
})
