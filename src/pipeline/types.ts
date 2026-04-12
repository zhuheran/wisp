export interface PayloadItem {
  type: 'text' | 'image' | 'resource'
  text?: string
  data?: string
  mimeType?: string
  uri?: string
  blob?: string
}

export interface DetectionResult {
  kind: 'text' | 'image_base64' | 'image_url' | 'binary_resource' | 'unknown'
  mimeType: string | null
  sizeBytes: number
  needsCompression: boolean
  needsPrefixFix: boolean
  isBase64: boolean
}

export interface TransformResult {
  type: 'text' | 'image_url'
  text?: string
  imageUrl?: {
    url: string
  }
  originalSizeBytes: number
  transformedSizeBytes: number
  wasCompressed: boolean
}

export interface VisionRouteResult {
  content: VisionContent
  fallbackUsed: boolean
  fallbackReason?: string
}

export type VisionContent =
  | { type: 'image_url'; image_url: { url: string } }
  | { type: 'text'; text: string }

export interface PipelineConfig {
  compressionThresholdBytes: number
  maxPayloadBytes: number
  jpegQuality: number
  maxWidth: number
  maxHeight: number
  mimeWhitelist: string[]
  enableCompression: boolean
  tempUrlEndpoint?: string
}

export const DEFAULT_PIPELINE_CONFIG: PipelineConfig = {
  compressionThresholdBytes: 4 * 1024 * 1024,
  maxPayloadBytes: 20 * 1024 * 1024,
  jpegQuality: 80,
  maxWidth: 2048,
  maxHeight: 2048,
  mimeWhitelist: [
    'image/png',
    'image/jpeg',
    'image/gif',
    'image/webp',
    'image/svg+xml',
    'image/bmp',
    'image/tiff',
  ],
  enableCompression: true,
}
