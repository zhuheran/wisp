import { describe, expect, it } from 'vitest'
import type { Model, Provider } from '../../libs/types'
import {
  appendNewModels,
  sanitizeProviderId,
  uniqueProviderId,
} from '../../utils/provider'

const providers = (names: string[]): Provider[] => names.map((name) => ({
  name,
  display_name: name,
  base_url: '',
  models: [],
}))

const model = (name: string, displayName = name): Model => ({
  metadata: { name, display_name: displayName },
  model_info: { type: 'audio' },
})

describe('sanitizeProviderId', () => {
  it('trims, lowercases, and separates words', () => {
    expect(sanitizeProviderId('  OpenAI Compatible  ')).toBe('openai-compatible')
  })

  it('collapses punctuation and repeated separators', () => {
    expect(sanitizeProviderId('My___Provider / v2')).toBe('my-provider-v2')
  })

  it('falls back when the display name has no ASCII ID characters', () => {
    expect(sanitizeProviderId('中文提供商')).toBe('provider')
    expect(sanitizeProviderId('   ')).toBe('provider')
  })
})

describe('uniqueProviderId', () => {
  it('adds numeric suffixes without changing the base slug', () => {
    expect(uniqueProviderId('OpenAI', providers(['openai', 'openai-2']))).toBe('openai-3')
  })
})

describe('appendNewModels', () => {
  it('keeps the complete local model when a fetched model has the same name', () => {
    const local = model('gpt-4', 'My GPT')
    const fetched = model('gpt-4', 'Remote GPT')
    const result = appendNewModels([local], [fetched, model('new-model')])

    expect(result).toEqual([local, model('new-model')])
    expect(result[0]).toBe(local)
  })

  it('does not mutate input arrays', () => {
    const existing = [model('local')]
    const fetched = [model('remote')]
    const result = appendNewModels(existing, fetched)

    expect(existing).toHaveLength(1)
    expect(fetched).toHaveLength(1)
    expect(result).not.toBe(existing)
  })

  it('adds each fetched model ID at most once', () => {
    const result = appendNewModels([], [model('remote'), model('remote', 'Duplicate')])

    expect(result).toHaveLength(1)
    expect(result[0].metadata.display_name).toBe('remote')
  })
})
