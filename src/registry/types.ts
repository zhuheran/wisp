export interface NormalizedTool {
  name: string
  serverId: string
  qualifiedName: string
  description?: string
  inputSchema: {
    type: 'object'
    properties?: Record<string, NormalizedProperty>
    required?: string[]
  }
  annotations?: {
    title?: string
    readOnlyHint?: boolean
    destructiveHint?: boolean
    idempotentHint?: boolean
    openWorldHint?: boolean
  }
}

export interface NormalizedProperty {
  type: string
  description?: string
  default?: unknown
  enum?: string[]
  items?: NormalizedProperty
  properties?: Record<string, NormalizedProperty>
  required?: string[]
  anyOf?: NormalizedProperty[]
  oneOf?: NormalizedProperty[]
}

export interface ToolCallResult {
  serverId: string
  toolName: string
  content: ToolCallContent[]
  isError?: boolean
}

export type ToolCallContent =
  | { type: 'text'; text: string }
  | { type: 'image'; data: string; mimeType: string }
  | { type: 'resource'; uri: string; mimeType?: string; text?: string; blob?: string }

export interface ToolRegistryOptions {
  namespaceSeparator?: string
  refreshIntervalMs?: number
}
