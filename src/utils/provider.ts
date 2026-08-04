import type { Model, Provider } from '../libs/types'

export function sanitizeProviderId(displayName: string): string {
  const slug = displayName
    .trim()
    .toLowerCase()
    .replace(/[^a-z0-9]+/g, '-')
    .replace(/^-+|-+$/g, '')

  return slug || 'provider'
}

export function uniqueProviderId(displayName: string, providers: Provider[]): string {
  const base = sanitizeProviderId(displayName)
  const used = new Set(providers.map((provider) => provider.name))

  if (!used.has(base)) return base

  let suffix = 2
  while (used.has(`${base}-${suffix}`)) suffix += 1
  return `${base}-${suffix}`
}

export function appendNewModels(existing: Model[], fetched: Model[]): Model[] {
  const seenNames = new Set(existing.map((model) => model.metadata.name))
  const newModels: Model[] = []

  for (const model of fetched) {
    if (seenNames.has(model.metadata.name)) continue
    seenNames.add(model.metadata.name)
    newModels.push(model)
  }

  return [...existing, ...newModels]
}
