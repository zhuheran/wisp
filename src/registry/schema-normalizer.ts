import type { NormalizedProperty } from './types'

type JsonSchema = Record<string, unknown>

export function normalizeSchema(raw: JsonSchema): NormalizedProperty {
  const prop: NormalizedProperty = {
    type: coerceType(raw.type),
  }

  if (raw.description && typeof raw.description === 'string') {
    prop.description = raw.description
  }

  if ('default' in raw) {
    prop.default = raw.default
  }

  if (Array.isArray(raw.enum)) {
    prop.enum = raw.enum.map(String)
  }

  if (raw.type === 'array' && raw.items && typeof raw.items === 'object') {
    prop.items = normalizeSchema(raw.items as JsonSchema)
  }

  if (raw.type === 'object' && raw.properties && typeof raw.properties === 'object') {
    const props: Record<string, NormalizedProperty> = {}
    for (const [key, value] of Object.entries(raw.properties as Record<string, JsonSchema>)) {
      props[key] = normalizeSchema(value)
    }
    prop.properties = props

    if (Array.isArray(raw.required)) {
      prop.required = raw.required.map(String)
    }
  }

  if (Array.isArray(raw.anyOf)) {
    prop.anyOf = raw.anyOf.map((s: unknown) => normalizeSchema(s as JsonSchema))
  }

  if (Array.isArray(raw.oneOf)) {
    prop.oneOf = raw.oneOf.map((s: unknown) => normalizeSchema(s as JsonSchema))
  }

  return prop
}

function coerceType(type: unknown): string {
  if (typeof type !== 'string') return 'string'

  const normalized = type.toLowerCase().trim()
  switch (normalized) {
    case 'integer':
      return 'number'
    case 'boolean':
      return 'boolean'
    case 'number':
      return 'number'
    case 'string':
      return 'string'
    case 'array':
      return 'array'
    case 'object':
      return 'object'
    case 'null':
      return 'null'
    default:
      return 'string'
  }
}

export function normalizeInputSchema(
  schema: Record<string, unknown>,
): { type: 'object'; properties?: Record<string, NormalizedProperty>; required?: string[] } {
  const properties: Record<string, NormalizedProperty> = {}

  if (schema.properties && typeof schema.properties === 'object') {
    for (const [key, value] of Object.entries(schema.properties as Record<string, JsonSchema>)) {
      properties[key] = normalizeSchema(value)
    }
  }

  return {
    type: 'object',
    properties: Object.keys(properties).length > 0 ? properties : undefined,
    required: Array.isArray(schema.required) ? schema.required.map(String) : undefined,
  }
}
