import { describe, it, expect, beforeEach } from 'vitest'
import {
  hashDescription,
  loadDisplayNameCache,
  getCachedDisplayName,
  cacheDisplayNames,
} from '../../libs/toolDisplayNames'

describe('hashDescription', () => {
  it('is deterministic for the same input', () => {
    expect(hashDescription('hello')).toBe(hashDescription('hello'))
  })

  it('differs for different input', () => {
    expect(hashDescription('hello')).not.toBe(hashDescription('world'))
  })
})

describe('display name cache', () => {
  beforeEach(() => {
    const store = new Map<string, string>()
    ;(globalThis as unknown as { localStorage: Storage }).localStorage = {
      getItem: (k: string) => store.get(k) ?? null,
      setItem: (k: string, v: string) => { store.set(k, String(v)) },
      removeItem: (k: string) => { store.delete(k) },
      clear: () => { store.clear() },
      key: (_i: number) => null,
      length: 0,
    } as Storage
    localStorage.clear()
  })

  it('returns undefined when not cached', () => {
    expect(getCachedDisplayName('desc')).toBeUndefined()
  })

  it('round-trips a cached entry', () => {
    cacheDisplayNames({ [hashDescription('foo')]: 'Bar Do Thing' })
    expect(getCachedDisplayName('foo')).toBe('Bar Do Thing')
  })

  it('persists across reloads of the cache', () => {
    cacheDisplayNames({ foo: 'Bar Do Thing' })
    const fresh = loadDisplayNameCache()
    expect(fresh.foo).toBe('Bar Do Thing')
  })
})
