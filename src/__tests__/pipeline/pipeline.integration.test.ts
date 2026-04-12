import { describe, it, expect } from 'vitest'
import { detectPayload, isMimeWhitelisted, estimateBase64DecodedSize } from '../../pipeline/detector'
import { transformPayload } from '../../pipeline/transformer'
import { routeToVision, routeBatchToVision } from '../../pipeline/vision-router'
import type { PayloadItem, PipelineConfig } from '../../pipeline/types'
import { DEFAULT_PIPELINE_CONFIG } from '../../pipeline/types'

describe('detector', () => {
  describe('detectPayload', () => {
    it('should detect text content', () => {
      const item: PayloadItem = { type: 'text', text: 'Hello world' }
      const result = detectPayload(item)

      expect(result.kind).toBe('text')
      expect(result.mimeType).toBe('text/plain')
      expect(result.isBase64).toBe(false)
    })

    it('should detect image base64 content', () => {
      const item: PayloadItem = {
        type: 'image',
        data: 'iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAADUlEQVR42mNk+M9QDwADhgGAWjR9awAAAABJRU5ErkJggg==',
        mimeType: 'image/png',
      }
      const result = detectPayload(item)

      expect(result.kind).toBe('image_base64')
      expect(result.mimeType).toBe('image/png')
      expect(result.isBase64).toBe(true)
      expect(result.needsPrefixFix).toBe(true)
    })

    it('should detect image with data URI prefix', () => {
      const item: PayloadItem = {
        type: 'image',
        data: 'data:image/png;base64,iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAADUlEQVR42mNk+M9QDwADhgGAWjR9awAAAABJRU5ErkJggg==',
        mimeType: 'image/png',
      }
      const result = detectPayload(item)

      expect(result.kind).toBe('image_base64')
      expect(result.mimeType).toBe('image/png')
      expect(result.needsPrefixFix).toBe(false)
    })

    it('should detect image URL', () => {
      const item: PayloadItem = {
        type: 'image',
        data: 'https://example.com/image.png',
        mimeType: 'image/png',
      }
      const result = detectPayload(item)

      expect(result.kind).toBe('image_url')
      expect(result.isBase64).toBe(false)
    })

    it('should detect JPEG from base64 header', () => {
      const item: PayloadItem = {
        type: 'image',
        data: '/9j/4AAQSkZJRgABAQAAAQABAAD/2wBDAAgGBgcGBQgHBwcJCQgKDBQNDAsLDBkSEw8UHRofHh0aHBwgJC4nICIsIxwcKDcpLDAxNDQ0Hyc5PTgyPC4zNDL/wAALCAABAAEBAREA/8QAFAABAAAAAAAAAAAAAAAAAAAACf/EABQQAQAAAAAAAAAAAAAAAAAAAAD/2gAIAQEAAD8AVN//2Q==',
      }
      const result = detectPayload(item)

      expect(result.mimeType).toBe('image/jpeg')
    })

    it('should flag large payloads for compression', () => {
      const largeBase64 = 'A'.repeat(6 * 1024 * 1024)
      const item: PayloadItem = {
        type: 'image',
        data: largeBase64,
        mimeType: 'image/png',
      }
      const result = detectPayload(item)

      expect(result.needsCompression).toBe(true)
    })

    it('should detect resource with blob', () => {
      const item: PayloadItem = {
        type: 'resource',
        uri: 'file:///test.png',
        mimeType: 'image/png',
        blob: 'iVBORw0KGgo=',
      }
      const result = detectPayload(item)

      expect(result.kind).toBe('image_base64')
      expect(result.isBase64).toBe(true)
    })

    it('should detect resource with text', () => {
      const item: PayloadItem = {
        type: 'resource',
        uri: 'file:///test.txt',
        mimeType: 'text/plain',
        text: 'Hello',
      }
      const result = detectPayload(item)

      expect(result.kind).toBe('text')
    })
  })

  describe('isMimeWhitelisted', () => {
    it('should whitelist image/png', () => {
      expect(isMimeWhitelisted('image/png')).toBe(true)
    })

    it('should not whitelist application/pdf', () => {
      expect(isMimeWhitelisted('application/pdf')).toBe(false)
    })

    it('should return false for null', () => {
      expect(isMimeWhitelisted(null)).toBe(false)
    })
  })

  describe('estimateBase64DecodedSize', () => {
    it('should estimate decoded size', () => {
      expect(estimateBase64DecodedSize(100)).toBe(75)
      expect(estimateBase64DecodedSize(4)).toBe(3)
    })
  })
})

describe('transformer', () => {
  describe('transformPayload', () => {
    it('should transform text content', async () => {
      const item: PayloadItem = { type: 'text', text: 'Hello' }
      const result = await transformPayload(item)

      expect(result.type).toBe('text')
      expect(result.text).toBe('Hello')
    })

    it('should transform image URL content', async () => {
      const item: PayloadItem = {
        type: 'image',
        data: 'https://example.com/image.png',
        mimeType: 'image/png',
      }
      const result = await transformPayload(item)

      expect(result.type).toBe('image_url')
      expect(result.imageUrl?.url).toBe('https://example.com/image.png')
    })

    it('should transform base64 image to data URI', async () => {
      const item: PayloadItem = {
        type: 'image',
        data: 'iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAADUlEQVR42mNk+M9QDwADhgGAWjR9awAAAABJRU5ErkJggg==',
        mimeType: 'image/png',
      }
      const result = await transformPayload(item)

      expect(result.type).toBe('image_url')
      expect(result.imageUrl?.url).toMatch(/^data:image\/png;base64,/)
    })

    it('should fix missing data URI prefix', async () => {
      const item: PayloadItem = {
        type: 'image',
        data: 'iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAADUlEQVR42mNk+M9QDwADhgGAWjR9awAAAABJRU5ErkJggg==',
        mimeType: 'image/png',
      }
      const result = await transformPayload(item)

      expect(result.type).toBe('image_url')
      expect(result.imageUrl?.url).toContain('data:image/png;base64,')
    })

    it('should reject non-whitelisted MIME types', async () => {
      const item: PayloadItem = {
        type: 'image',
        data: 'AAAA',
        mimeType: 'application/exe',
      }
      const result = await transformPayload(item)

      expect(result.type).toBe('text')
      expect(result.text).toContain('not in whitelist')
    })

    it('should reject invalid base64', async () => {
      const item: PayloadItem = {
        type: 'image',
        data: 'not-valid-base64!!!',
        mimeType: 'image/png',
      }
      const result = await transformPayload(item)

      expect(result.type).toBe('text')
      expect(result.text).toContain('Invalid Base64')
    })

    it('should handle oversized payloads', async () => {
      const largeBase64 = 'A'.repeat(25 * 1024 * 1024)
      const item: PayloadItem = {
        type: 'image',
        data: largeBase64,
        mimeType: 'image/png',
      }

      const smallConfig: PipelineConfig = {
        ...DEFAULT_PIPELINE_CONFIG,
        maxPayloadBytes: 1024,
        enableCompression: false,
      }

      const result = await transformPayload(item, smallConfig)
      expect(result.type).toBe('text')
      expect(result.text).toContain('too large')
    })
  })
})

describe('vision-router', () => {
  describe('routeToVision', () => {
    it('should route text to text vision content', async () => {
      const item: PayloadItem = { type: 'text', text: 'Hello' }
      const result = await routeToVision(item)

      expect(result.content.type).toBe('text')
      expect(result.fallbackUsed).toBe(false)
    })

    it('should route image URL to image_url vision content', async () => {
      const item: PayloadItem = {
        type: 'image',
        data: 'https://example.com/image.png',
        mimeType: 'image/png',
      }
      const result = await routeToVision(item)

      expect(result.content.type).toBe('image_url')
      if (result.content.type === 'image_url') {
        expect(result.content.image_url.url).toBe('https://example.com/image.png')
      }
    })

    it('should route base64 image to image_url with data URI', async () => {
      const item: PayloadItem = {
        type: 'image',
        data: 'iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAADUlEQVR42mNk+M9QDwADhgGAWjR9awAAAABJRU5ErkJggg==',
        mimeType: 'image/png',
      }
      const result = await routeToVision(item)

      expect(result.content.type).toBe('image_url')
      if (result.content.type === 'image_url') {
        expect(result.content.image_url.url).toMatch(/^data:image\/png;base64,/)
      }
    })

    it('should fallback to text for non-whitelisted MIME', async () => {
      const item: PayloadItem = {
        type: 'image',
        data: 'AAAA',
        mimeType: 'application/exe',
      }
      const result = await routeToVision(item)

      expect(result.content.type).toBe('text')
      expect(result.fallbackUsed).toBe(true)
    })

    it('should route binary resource to text fallback', async () => {
      const item: PayloadItem = {
        type: 'resource',
        uri: 'file:///data.bin',
        mimeType: 'application/octet-stream',
        blob: 'AAAA',
      }
      const result = await routeToVision(item)

      expect(result.content.type).toBe('text')
      expect(result.fallbackUsed).toBe(true)
    })
  })

  describe('routeBatchToVision', () => {
    it('should process multiple items', async () => {
      const items: PayloadItem[] = [
        { type: 'text', text: 'Hello' },
        { type: 'image', data: 'https://example.com/img.png', mimeType: 'image/png' },
      ]

      const results = await routeBatchToVision(items)
      expect(results).toHaveLength(2)
      expect(results[0].content.type).toBe('text')
      expect(results[1].content.type).toBe('image_url')
    })
  })
})
