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
  compression_threshold_bytes: number
  max_payload_bytes: number
  jpeg_quality: number
  max_width: number
  max_height: number
  mime_whitelist: string[]
  enable_compression: boolean
  temp_url_endpoint?: string
}

export const DEFAULT_PIPELINE_CONFIG: PipelineConfig = {
  compression_threshold_bytes: 4 * 1024 * 1024,
  max_payload_bytes: 20 * 1024 * 1024,
  jpeg_quality: 80,
  max_width: 2048,
  max_height: 2048,
  mime_whitelist: [
    'image/png',
    'image/jpeg',
    'image/gif',
    'image/webp',
    'image/svg+xml',
    'image/bmp',
    'image/tiff',
  ],
  enable_compression: true,
}
