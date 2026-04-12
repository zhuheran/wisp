import type { PayloadItem, TransformResult, PipelineConfig } from './types'
import { DEFAULT_PIPELINE_CONFIG } from './types'
import { detectPayload, isMimeWhitelisted } from './detector'
import { invoke } from '@tauri-apps/api/core'

const DATA_URI_PREFIX_REGEX = /^data:([^;]+);base64,/i

interface ImageCompressConfig {
  max_width: number
  max_height: number
  jpeg_quality: number
}

interface ImageCompressResult {
  data: string
  mime_type: string
  original_size: number
  compressed_size: number
  was_compressed: boolean
}

export async function transformPayload(
  item: PayloadItem,
  config: PipelineConfig = DEFAULT_PIPELINE_CONFIG,
): Promise<TransformResult> {
  const detection = detectPayload(item, config)

  if (detection.kind === 'text') {
    return {
      type: 'text',
      text: item.text ?? '',
      originalSizeBytes: detection.sizeBytes,
      transformedSizeBytes: detection.sizeBytes,
      wasCompressed: false,
    }
  }

  if (detection.kind === 'image_url') {
    const url = item.type === 'image' ? item.data ?? '' : (item.type === 'resource' ? item.uri ?? '' : '')
    return {
      type: 'image_url',
      imageUrl: { url },
      originalSizeBytes: detection.sizeBytes,
      transformedSizeBytes: detection.sizeBytes,
      wasCompressed: false,
    }
  }

  if (detection.kind === 'image_base64' || detection.kind === 'binary_resource') {
    if (!isMimeWhitelisted(detection.mimeType, config)) {
      return {
        type: 'text',
        text: `[Binary content: ${detection.mimeType ?? 'unknown'} (${formatBytes(detection.sizeBytes)}) - MIME type not in whitelist]`,
        originalSizeBytes: detection.sizeBytes,
        transformedSizeBytes: 0,
        wasCompressed: false,
      }
    }

    if (!detection.isBase64) {
      return {
        type: 'text',
        text: `[Invalid Base64 data for ${detection.mimeType ?? 'unknown'} content]`,
        originalSizeBytes: detection.sizeBytes,
        transformedSizeBytes: 0,
        wasCompressed: false,
      }
    }

    let base64Data = extractBase64Data(item)
    let wasCompressed = false
    let mimeType = detection.mimeType ?? 'image/png'

    if (detection.needsCompression && config.enableCompression) {
      const compressed = await compressImageViaBackend(base64Data, mimeType, config)
      if (compressed && compressed.was_compressed) {
        base64Data = compressed.data
        mimeType = compressed.mime_type
        wasCompressed = true
      }
    }

    if (detection.sizeBytes > config.maxPayloadBytes && !wasCompressed) {
      return {
        type: 'text',
        text: `[Image too large: ${formatBytes(detection.sizeBytes)} exceeds ${formatBytes(config.maxPayloadBytes)} limit]`,
        originalSizeBytes: detection.sizeBytes,
        transformedSizeBytes: 0,
        wasCompressed: false,
      }
    }

    const dataUri = `data:${mimeType};base64,${base64Data}`
    const transformedSize = Math.ceil(base64Data.length * 3 / 4)

    return {
      type: 'image_url',
      imageUrl: { url: dataUri },
      originalSizeBytes: detection.sizeBytes,
      transformedSizeBytes: transformedSize,
      wasCompressed,
    }
  }

  return {
    type: 'text',
    text: `[Unsupported content type]`,
    originalSizeBytes: detection.sizeBytes,
    transformedSizeBytes: 0,
    wasCompressed: false,
  }
}

function extractBase64Data(item: PayloadItem): string {
  let raw = ''

  if (item.type === 'image') {
    raw = item.data ?? ''
  } else if (item.type === 'resource') {
    raw = item.blob ?? ''
  }

  const prefixMatch = raw.match(DATA_URI_PREFIX_REGEX)
  if (prefixMatch) {
    return raw.slice(prefixMatch[0].length).replace(/\s/g, '')
  }

  return raw.replace(/\s/g, '')
}

async function compressImageViaBackend(
  base64Data: string,
  mimeType: string,
  config: PipelineConfig,
): Promise<ImageCompressResult | null> {
  try {
    const compressConfig: ImageCompressConfig = {
      max_width: config.maxWidth,
      max_height: config.maxHeight,
      jpeg_quality: config.jpegQuality,
    }

    const result = await invoke<ImageCompressResult>('compress_image', {
      base64Data,
      mimeType,
      config: compressConfig,
    })

    return result
  } catch (e) {
    console.error('Failed to compress image via backend:', e)
    return null
  }
}

function formatBytes(bytes: number): string {
  if (bytes < 1024) return `${bytes} B`
  if (bytes < 1024 * 1024) return `${(bytes / 1024).toFixed(1)} KB`
  return `${(bytes / (1024 * 1024)).toFixed(1)} MB`
}
