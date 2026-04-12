import { describe, it, expect, vi, beforeEach } from 'vitest'
import { ToolRegistry } from '../../registry/tool-registry'
import { normalizeSchema, normalizeInputSchema } from '../../registry/schema-normalizer'

describe('schema-normalizer', () => {
  describe('normalizeSchema', () => {
    it('should normalize string type', () => {
      const result = normalizeSchema({ type: 'string' })
      expect(result.type).toBe('string')
    })

    it('should coerce integer to number', () => {
      const result = normalizeSchema({ type: 'integer' })
      expect(result.type).toBe('number')
    })

    it('should normalize object with properties', () => {
      const result = normalizeSchema({
        type: 'object',
        properties: {
          name: { type: 'string', description: 'The name' },
          age: { type: 'integer' },
        },
        required: ['name'],
      })

      expect(result.type).toBe('object')
      expect(result.properties).toBeDefined()
      expect(result.properties!['name'].type).toBe('string')
      expect(result.properties!['name'].description).toBe('The name')
      expect(result.properties!['age'].type).toBe('number')
      expect(result.required).toEqual(['name'])
    })

    it('should normalize array with items', () => {
      const result = normalizeSchema({
        type: 'array',
        items: { type: 'string' },
      })

      expect(result.type).toBe('array')
      expect(result.items).toBeDefined()
      expect(result.items!.type).toBe('string')
    })

    it('should handle enum values', () => {
      const result = normalizeSchema({
        type: 'string',
        enum: ['a', 'b', 'c'],
      })

      expect(result.enum).toEqual(['a', 'b', 'c'])
    })

    it('should handle anyOf', () => {
      const result = normalizeSchema({
        anyOf: [
          { type: 'string' },
          { type: 'number' },
        ],
      })

      expect(result.anyOf).toHaveLength(2)
      expect(result.anyOf![0].type).toBe('string')
      expect(result.anyOf![1].type).toBe('number')
    })

    it('should default to string for unknown types', () => {
      const result = normalizeSchema({ type: 'unknown_type' })
      expect(result.type).toBe('string')
    })

    it('should default to string when type is missing', () => {
      const result = normalizeSchema({})
      expect(result.type).toBe('string')
    })
  })

  describe('normalizeInputSchema', () => {
    it('should produce a valid object schema', () => {
      const result = normalizeInputSchema({
        type: 'object',
        properties: {
          query: { type: 'string', description: 'Search query' },
          limit: { type: 'integer', default: 10 },
        },
        required: ['query'],
      })

      expect(result.type).toBe('object')
      expect(result.properties).toBeDefined()
      expect(result.properties!['query'].type).toBe('string')
      expect(result.properties!['limit'].type).toBe('number')
      expect(result.properties!['limit'].default).toBe(10)
      expect(result.required).toEqual(['query'])
    })

    it('should handle empty properties', () => {
      const result = normalizeInputSchema({ type: 'object' })
      expect(result.type).toBe('object')
      expect(result.properties).toBeUndefined()
    })
  })
})

describe('ToolRegistry', () => {
  let registry: ToolRegistry

  beforeEach(() => {
    const mockConnectionManager = {
      getTransport: vi.fn(),
      getConnectedServerIds: vi.fn().mockReturnValue([]),
    } as any

    registry = new ToolRegistry(mockConnectionManager)
  })

  it('should start with no tools', () => {
    expect(registry.getAllTools()).toHaveLength(0)
  })

  it('should find no tools for empty query', () => {
    expect(registry.findTools('test')).toHaveLength(0)
  })

  it('should clear server tools', () => {
    registry.clearServer('nonexistent')
    expect(registry.getToolsByServer('nonexistent')).toHaveLength(0)
  })

  it('should throw when executing unknown tool', async () => {
    await expect(registry.executeTool('unknown__tool')).rejects.toThrow()
  })

  it('should destroy cleanly', () => {
    registry.destroy()
    expect(registry.getAllTools()).toHaveLength(0)
  })
})
