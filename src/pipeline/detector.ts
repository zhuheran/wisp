import type { PayloadItem, DetectionResult, PipelineConfig } from './types'
import { DEFAULT_PIPELINE_CONFIG } from './types'

const BASE64_REGEX = /^[A-Za-z0-9+/]*={0,2}$/
const DATA_URI_PREFIX_REGEX = /^data:([^;]+);base64,/
const URL_REGEX = /^https?:\/\//

export function detectPayload(item: PayloadItem, config: PipelineConfig = DEFAULT_PIPELINE_CONFIG): DetectionResult {
  if (item.type === 'text') {
    return {
      kind: 'text',
      mimeType: 'text/plain',
      sizeBytes: new TextEncoder().encode(item.text ?? '').byteLength,
      needsCompression: false,
      needsPrefixFix: false,
      isBase64: false,
    }
  }

  if (item.type === 'resource') {
    if (item.blob) {
      return detectBase64Content(item.blob, item.mimeType, config)
    }
    if (item.text) {
      return {
        kind: 'text',
        mimeType: item.mimeType ?? 'text/plain',
        sizeBytes: new TextEncoder().encode(item.text).byteLength,
        needsCompression: false,
        needsPrefixFix: false,
        isBase64: false,
      }
    }
    return {
      kind: 'unknown',
      mimeType: item.mimeType ?? null,
      sizeBytes: 0,
      needsCompression: false,
      needsPrefixFix: false,
      isBase64: false,
    }
  }

  if (item.type === 'image') {
    return detectBase64Content(item.data ?? '', item.mimeType, config)
  }

  return {
    kind: 'unknown',
    mimeType: null,
    sizeBytes: 0,
    needsCompression: false,
    needsPrefixFix: false,
    isBase64: false,
  }
}

function detectBase64Content(raw: string, declaredMime: string | undefined, config: PipelineConfig): DetectionResult {
  if (URL_REGEX.test(raw.trim())) {
    return {
      kind: 'image_url',
      mimeType: declaredMime ?? null,
      sizeBytes: raw.length,
      needsCompression: false,
      needsPrefixFix: false,
      isBase64: false,
    }
  }

  const prefixMatch = raw.match(DATA_URI_PREFIX_REGEX)
  let base64Data = raw
  let detectedMime = declaredMime ?? null
  let needsPrefixFix = false

  if (prefixMatch) {
    detectedMime = prefixMatch[1]
    base64Data = raw.slice(prefixMatch[0].length)
  } else {
    needsPrefixFix = true
    if (!detectedMime) {
      detectedMime = guessMimeTypeFromBase64(base64Data)
    }
  }

  const cleanBase64 = base64Data.replace(/\s/g, '')
  const isBase64 = BASE64_REGEX.test(cleanBase64) && cleanBase64.length > 0
  const sizeBytes = isBase64 ? Math.ceil(cleanBase64.length * 3 / 4) : raw.length

  const isImage = detectedMime?.startsWith('image/') ?? false

  return {
    kind: isImage ? 'image_base64' : (isBase64 ? 'binary_resource' : 'unknown'),
    mimeType: detectedMime,
    sizeBytes,
    needsCompression: sizeBytes > config.compressionThresholdBytes,
    needsPrefixFix,
    isBase64,
  }
}

function guessMimeTypeFromBase64(data: string): string {
  const trimmed = data.trimStart()

  if (trimmed.startsWith('\x89PNG')) return 'image/png'
  if (trimmed.startsWith('/9j/')) return 'image/jpeg'
  if (trimmed.startsWith('R0lGOD')) return 'image/gif'
  if (trimmed.startsWith('UklGR')) return 'image/webp'
  if (trimmed.startsWith('iVBOR')) return 'image/png'
  if (trimmed.startsWith('PHN2Zy')) return 'image/svg+xml'
  if (trimmed.startsWith('Qk')) return 'image/bmp'

  return 'application/octet-stream'
}

export function isMimeWhitelisted(mimeType: string | null, config: PipelineConfig = DEFAULT_PIPELINE_CONFIG): boolean {
  if (!mimeType) return false
  return config.mimeWhitelist.includes(mimeType)
}

export function estimateBase64DecodedSize(base64Length: number): number {
  return Math.ceil(base64Length * 3 / 4)
}
