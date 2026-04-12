import type { PayloadItem, VisionRouteResult, PipelineConfig } from './types'
import { DEFAULT_PIPELINE_CONFIG } from './types'
import { detectPayload } from './detector'
import { transformPayload } from './transformer'

export async function routeToVision(
  item: PayloadItem,
  config: PipelineConfig = DEFAULT_PIPELINE_CONFIG,
): Promise<VisionRouteResult> {
  const detection = detectPayload(item, config)

  if (detection.kind === 'text') {
    return {
      content: { type: 'text', text: item.text ?? '' },
      fallbackUsed: false,
    }
  }

  if (detection.kind === 'image_url') {
    const url = item.type === 'image' ? item.data ?? '' : (item.type === 'resource' ? item.uri ?? '' : '')
    return {
      content: { type: 'image_url', image_url: { url } },
      fallbackUsed: false,
    }
  }

  if (detection.kind === 'image_base64') {
    try {
      const result = await transformPayload(item, config)

      if (result.type === 'image_url' && result.imageUrl) {
        return {
          content: { type: 'image_url', image_url: { url: result.imageUrl.url } },
          fallbackUsed: false,
        }
      }

      if (result.type === 'text' && result.text) {
        if (detection.sizeBytes > config.maxPayloadBytes) {
          const tempUrl = await generateTempUrl(item, config)
          if (tempUrl) {
            return {
              content: { type: 'image_url', image_url: { url: tempUrl } },
              fallbackUsed: true,
              fallbackReason: 'Payload exceeds max size, using temp URL',
            }
          }
        }

        return {
          content: { type: 'text', text: result.text },
          fallbackUsed: true,
          fallbackReason: result.text,
        }
      }
    } catch (error) {
      return {
        content: {
          type: 'text',
          text: `[Image processing failed: ${error instanceof Error ? error.message : 'unknown error'}]`,
        },
        fallbackUsed: true,
        fallbackReason: 'Transform error',
      }
    }
  }

  if (detection.kind === 'binary_resource') {
    if (detection.mimeType?.startsWith('image/')) {
      try {
        const result = await transformPayload(item, config)
        if (result.type === 'image_url' && result.imageUrl) {
          return {
            content: { type: 'image_url', image_url: { url: result.imageUrl.url } },
            fallbackUsed: false,
          }
        }
      } catch {
        // fall through to text fallback
      }
    }

    return {
      content: {
        type: 'text',
        text: `[Binary resource: ${detection.mimeType ?? 'unknown'} (${formatBytes(detection.sizeBytes)})]`,
      },
      fallbackUsed: true,
      fallbackReason: 'Non-image binary resource',
    }
  }

  return {
    content: { type: 'text', text: '[Unsupported content]' },
    fallbackUsed: true,
    fallbackReason: 'Unknown content kind',
  }
}

export async function routeBatchToVision(
  items: PayloadItem[],
  config: PipelineConfig = DEFAULT_PIPELINE_CONFIG,
): Promise<VisionRouteResult[]> {
  return Promise.all(items.map((item) => routeToVision(item, config)))
}

async function generateTempUrl(_item: PayloadItem, config: PipelineConfig): Promise<string | null> {
  if (!config.tempUrlEndpoint) return null

  // Simulated temp URL generation - in production this would upload to a storage service
  const id = crypto.randomUUID()
  return `${config.tempUrlEndpoint}/${id}`
}

function formatBytes(bytes: number): string {
  if (bytes < 1024) return `${bytes} B`
  if (bytes < 1024 * 1024) return `${(bytes / 1024).toFixed(1)} KB`
  return `${(bytes / (1024 * 1024)).toFixed(1)} MB`
}
