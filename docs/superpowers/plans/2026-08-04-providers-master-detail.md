# Provider Management Master-Detail Redesign Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Refactor Provider management into a SkillsView-aligned page header plus a two-column master-detail workspace, with stable generated Provider IDs and non-destructive model synchronization.

**Architecture:** `ProvidersView.vue` owns the page header, selection, add-provider flow, deletion confirmation, and master-detail layout. `ProviderList.vue` becomes a navigation-only view; `ProviderConfig.vue` composes identity, settings, and models. Pure Provider ID and model-merge logic lives in `src/utils/provider.ts` so it can be tested independently of Naive UI and Tauri.

**Tech Stack:** Vue 3 `<script setup>`, TypeScript, Pinia, Naive UI, Fluent icons, Vitest, Vue Test Utils, Tauri invoke commands.

## Global Constraints

- Use `SkillsView.vue` only as the visual baseline; do not modify Skills files or add Skill detail commands.
- Preserve the in-progress rig migration changes in the working tree; do not revert unrelated modifications.
- `Provider.name` is a stable internal ID generated from `display_name` at creation and is never editable afterward.
- API keys remain outside the serialized Provider and use `getCredential` / `setCredential` with the stable Provider ID.
- `ModelInfo` is a discriminated union; access `configs.capabilities` only inside the `text_generation` branch.
- Model synchronization only appends remote models whose `metadata.name` is not already configured; same-name local models are never overwritten.
- Do not add dependencies or backend commands.
- Keep visible focus states, logical keyboard order, labelled form controls, loading feedback, and accessible names for icon-only buttons.
- Use existing Naive UI theme variables; do not introduce raw component-level hex colors.

---

## File Map

### Create

- `src/utils/provider.ts` — pure Provider ID generation and non-destructive model merge helpers.
- `src/components/AddProviderDialog.vue` — controlled create-provider form; emits Provider payload and API key without writing to stores.
- `src/__tests__/utils/provider.test.ts` — unit tests for sanitization, collision handling, and model merge behavior.

### Modify

- `src/views/ProvidersView.vue` — page header, master-detail layout, selection, create/delete orchestration, empty states.
- `src/components/ProviderList.vue` — navigation-only Provider list; remove embedded add modal and store mutation for creation.
- `src/components/ProviderConfig.vue` — right-side identity header and section composition.
- `src/components/ProviderDetailForm.vue` — stable ID display, labelled editing form, separate API-key save feedback, accessible actions.
- `src/components/ModelTable.vue` — append-only model sync, metadata-aware table, accessible action labels/tooltips, empty state.
- `src/stores/provider.ts` — preserve selection after reload and fix the missing `await` in `updateModel` while keeping existing command boundaries.

### Test / validation targets

- `src/__tests__/utils/provider.test.ts`
- `npm run build`
- `npm test`
- `cargo test -p wisp-configs`
- `cargo test -p wisp-llm`

---

## Task 1: Add pure Provider ID and model synchronization helpers

**Files:**
- Create: `src/utils/provider.ts`
- Create: `src/__tests__/utils/provider.test.ts`

**Interfaces:**

```ts
import type { Model, Provider } from '../libs/types'

export function sanitizeProviderId(displayName: string): string
export function uniqueProviderId(displayName: string, providers: Provider[]): string
export function appendNewModels(existing: Model[], fetched: Model[]): Model[]
```

`appendNewModels` must return the original existing model objects unchanged, followed by fetched models whose `metadata.name` is not already present. It must not mutate either input array.

- [ ] **Step 1: Write failing unit tests for Provider ID sanitization.**

```ts
import { describe, expect, it } from 'vitest'
import { sanitizeProviderId, uniqueProviderId } from '../../utils/provider'
import type { Provider } from '../../libs/types'

const providers = (names: string[]): Provider[] => names.map((name) => ({
  name,
  display_name: name,
  base_url: '',
  models: [],
}))

describe('sanitizeProviderId', () => {
  it('trims, lowercases, and separates words', () => {
    expect(sanitizeProviderId('  OpenAI Compatible  ')).toBe('openai-compatible')
  })

  it('collapses punctuation and repeated separators', () => {
    expect(sanitizeProviderId('My___Provider / v2')).toBe('my-provider-v2')
  })

  it('falls back when the display name has no ASCII ID characters', () => {
    expect(sanitizeProviderId('中文提供商')).toBe('provider')
    expect(sanitizeProviderId('   ')).toBe('provider')
  })
})

describe('uniqueProviderId', () => {
  it('adds numeric suffixes without changing the base slug', () => {
    expect(uniqueProviderId('OpenAI', providers(['openai', 'openai-2']))).toBe('openai-3')
  })
})
```

- [ ] **Step 2: Run the focused test and verify it fails because the helper module does not exist.**

Run: `npm test -- src/__tests__/utils/provider.test.ts`

Expected: FAIL with a module resolution error for `../../utils/provider`.

- [ ] **Step 3: Write the minimal pure implementation.**

```ts
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
  const existingNames = new Set(existing.map((model) => model.metadata.name))
  return [
    ...existing,
    ...fetched.filter((model) => !existingNames.has(model.metadata.name)),
  ]
}
```

- [ ] **Step 4: Add tests proving append-only model behavior and immutability.**

```ts
import { appendNewModels } from '../../utils/provider'
import type { Model } from '../../libs/types'

const model = (name: string, displayName = name): Model => ({
  metadata: { name, display_name: displayName },
  model_info: { type: 'audio' },
})

it('keeps the complete local model when a fetched model has the same name', () => {
  const local = model('gpt-4', 'My GPT')
  const fetched = model('gpt-4', 'Remote GPT')
  const result = appendNewModels([local], [fetched, model('new-model')])

  expect(result).toEqual([local, model('new-model')])
  expect(result[0]).toBe(local)
})

it('does not mutate input arrays', () => {
  const existing = [model('local')]
  const fetched = [model('remote')]
  const result = appendNewModels(existing, fetched)

  expect(existing).toHaveLength(1)
  expect(fetched).toHaveLength(1)
  expect(result).not.toBe(existing)
})
```

- [ ] **Step 5: Run the focused tests and confirm they pass.**

Run: `npm test -- src/__tests__/utils/provider.test.ts`

Expected: all sanitization, collision, append-only, and immutability tests pass.

---

## Task 2: Create the controlled Add Provider dialog

**Files:**
- Create: `src/components/AddProviderDialog.vue`

**Interfaces:**

```ts
import type { ApiType, Provider } from '../libs/types'

type AddProviderPayload = {
  provider: Provider
  apiKey: string
}

const props = defineProps<{
  show: boolean
  providers: Provider[]
  loading?: boolean
}>()

const emit = defineEmits<{
  'update:show': [show: boolean]
  save: [payload: AddProviderPayload]
}>()
```

The dialog owns field state and local validation, but it must not call Pinia or Tauri commands directly.

- [ ] **Step 1: Add the form component with visible labels and the generated ID preview.**

Use `n-modal`, `n-card`, `n-form`, `n-form-item`, `n-input`, `n-select`, and `n-button`. The fields are `display_name`, `api_type`, `base_url`, and optional `api_key`. Show a read-only `Provider ID` preview computed with `uniqueProviderId(displayName, providers)`. Do not render an editable `name` input.

The emitted Provider must have this shape:

```ts
{
  name: uniqueProviderId(displayName, props.providers),
  display_name: displayName.trim(),
  base_url: baseUrl.trim(),
  api_type,
  models: [],
}
```

- [ ] **Step 2: Add validation and reset behavior.**

Reject an empty trimmed Display name near the field. Keep the dialog open after validation failure or failed parent save. Reset fields after the parent reports successful save by watching `show` transition from `true` to `false` only when the parent has closed it; do not clear fields while the user is correcting an error.

- [ ] **Step 3: Add loading and keyboard behavior.**

Disable all form controls while `loading` is true, submit from the form’s Enter action, keep Cancel available when not loading, and use visible focus styles supplied by Naive UI. The primary button must show `loading`.

- [ ] **Step 4: Run the build to catch component/type errors before integrating it.**

Run: `npm run build`

Expected: the new component compiles; any failures must be limited to pre-existing workspace migration issues or be fixed before continuing.

---

## Task 3: Refactor ProviderList into navigation-only UI

**Files:**
- Modify: `src/components/ProviderList.vue`

**Interfaces:**

```ts
const props = defineProps<{
  selected: string | null
}>()

const emit = defineEmits<{
  'update:selected': [name: string]
  delete: [provider: Provider]
}>()
```

- [ ] **Step 1: Remove creation state and imports.**

Delete the embedded `showAddProvider`, `newProvider`, `apiTypeOptions`, `handleAddProvider`, `NModal`, `NInput`, `NSelect`, and `NCard` creation code. The component must no longer call `createProvider`.

- [ ] **Step 2: Render the Provider navigation list from the store.**

Each item must show `display_name`, a human-readable API type, and `models.length`. Use `props.selected === provider.name` for selection. Emit `update:selected` on click and Enter. Add `role="option"`, `aria-selected`, `tabindex="0"`, and a visible focus style.

- [ ] **Step 3: Keep deletion as an intent event rather than mutating from the list.**

On context-menu, prevent the browser menu and emit `delete` with the Provider. Do not add a second delete button to every row; the right-click behavior remains compatible with the existing desktop interaction while the parent owns confirmation and mutation.

- [ ] **Step 4: Add a list summary and empty state.**

Show the configured Provider count near the list heading. If the list is empty, render `n-empty` with text directing the user to the page-level Add provider action. Remove the bottom Add Provider button because creation is now owned by `ProvidersView.vue`.

- [ ] **Step 5: Run the focused utility tests and build.**

Run: `npm test -- src/__tests__/utils/provider.test.ts && npm run build`

Expected: utility tests pass and the list refactor compiles before the parent is changed.

---

## Task 4: Refactor ProviderConfig and ProviderDetailForm around the migrated contract

**Files:**
- Modify: `src/components/ProviderConfig.vue`
- Modify: `src/components/ProviderDetailForm.vue`

**Interfaces:**

`ProviderConfig.vue` continues to consume:

```ts
const props = defineProps<{ provider: Provider }>()
```

- [ ] **Step 1: Add the Provider identity section in `ProviderConfig.vue`.**

Render the selected `provider.display_name`, a human-readable API-type label, the model count, and a low-emphasis `ID: ${provider.name}`. Compose `ProviderDetailForm` under a `Provider settings` section and `ModelTable` under a `Models` section. Keep the content scrollable without nesting an unnecessary full-height `n-space`.

- [ ] **Step 2: Make ProviderDetailForm treat `name` as immutable metadata.**

Keep `name` out of editable form controls. Show it as read-only metadata if useful, but never bind it to a writable input. Continue cloning `props.provider` for edit state and preserve the existing cancel/reset behavior.

- [ ] **Step 3: Preserve separate API-key handling.**

Continue loading the key with `getCredential(props.provider.name)`. On save, call `updateProvider` for Provider fields and call `setCredential` only when the key changed and is non-empty. Surface a clear message if Provider update succeeds but API-key persistence fails; do not claim the complete operation succeeded.

- [ ] **Step 4: Improve form labels and action accessibility.**

Use top labels or explicit labels for Display name, Base URL, API type, and API key. Add tooltip/ARIA text to edit and save icon buttons. Add loading state to save while awaiting the store operation. Keep API key masked by default.

- [ ] **Step 5: Run the build.**

Run: `npm run build`

Expected: Provider identity and settings compile against the current `Provider`, `ApiType`, and keyring command signatures.

---

## Task 5: Make ModelTable synchronization append-only and metadata-aware

**Files:**
- Modify: `src/components/ModelTable.vue`
- Modify: `src/stores/provider.ts`

**Interfaces:**

Use the helper from Task 1:

```ts
import { appendNewModels } from '../utils/provider'
```

- [ ] **Step 1: Replace the current destructive merge.**

Change `handleFetch` so it calls `providerFetchModels(props.provider.name)`, computes `merged = appendNewModels(props.provider.models, fetched)`, and only calls `store.updateProvider` when `merged.length !== props.provider.models.length`. The existing local model objects must be passed through unchanged.

```ts
const fetched = await providerFetchModels(props.provider.name)
const merged = appendNewModels(props.provider.models, fetched)
const addedCount = merged.length - props.provider.models.length

if (addedCount > 0) {
  await store.updateProvider(props.provider.name, {
    ...props.provider,
    models: merged,
  })
}

message.info(
  addedCount > 0
    ? `Added ${addedCount} new model${addedCount === 1 ? '' : 's'}`
    : 'All fetched models are already configured',
)
```

- [ ] **Step 2: Make the table reflect the actual discriminated union.**

Keep `metadata.name`, `metadata.display_name`, `metadata.owned_by`, and `model_info.type` as separate columns. Add a formatted context-window column that renders `metadata.context_window` as tokens (for example `128k`) and uses an em dash when absent. Render capabilities only when `row.model_info.type === 'text_generation'`; never read `configs.capabilities` for other variants.

- [ ] **Step 3: Improve table empty/loading/action feedback.**

Use a clear empty description when no models are configured. Keep the fetch button loading while the request is active. Add Naive UI tooltips and `aria-label` values for fetch, add, edit, and delete icon-only controls. Preserve delete confirmation and drawer editing.

- [ ] **Step 4: Fix the existing missing await in `updateModel`.**

In `src/stores/provider.ts`, change:

```ts
configsUpdateModel(providerName, modelName, model)
```

to:

```ts
await configsUpdateModel(providerName, modelName, model)
```

Keep the existing reload in `finally` so the UI reflects the persisted model.

- [ ] **Step 5: Run focused utility tests and build.**

Run: `npm test -- src/__tests__/utils/provider.test.ts && npm run build`

Expected: append-only tests pass and the model table compiles without unsafe access to the ModelInfo union.

---

## Task 6: Rebuild ProvidersView as the page-level workspace

**Files:**
- Modify: `src/views/ProvidersView.vue`

**Interfaces:**

Use the existing store:

```ts
const providerStore = useProviderStore()
const selectedProvider = computed(() => providerStore.currentProvider)
```

`ProviderList` receives `:selected="selectedProvider?.name ?? null"` and emits selection. `AddProviderDialog` receives `:providers="providerStore.providers"` and emits the payload defined in Task 2.

- [ ] **Step 1: Add the SkillsView-aligned page header.**

Render a `Providers` title, `Manage API providers and available models` subtitle, and a primary `Add provider` button with a visible label and provider-add icon. Keep the header fixed while workspace panels scroll.

- [ ] **Step 2: Add the two-column master-detail workspace.**

Use `n-split` below the header. Keep the left panel as `ProviderList`; render `ProviderConfig` when `selectedProvider` exists. Render a right-side empty state when no Provider is selected, with distinct copy for zero Providers versus an existing Provider list.

- [ ] **Step 3: Orchestrate Provider creation.**

When AddProviderDialog emits `{ provider, apiKey }`:

```ts
const handleCreate = async ({ provider, apiKey }: AddProviderPayload) => {
  try {
    await providerStore.createProvider(provider)
    let keyError: unknown = null
    if (apiKey) {
      try {
        await setCredential(provider.name, apiKey)
      } catch (error) {
        keyError = error
      }
    }
    await providerStore.loadProviders()
    providerStore.selectProvider(provider.name)
    showAddProvider.value = false
    if (keyError) {
      message.warning(`Provider created, but the API key could not be saved: ${keyError}`)
    } else {
      message.success('Provider added')
    }
  } catch (error) {
    message.error(`Failed to add provider: ${error}`)
  }
}
```

Import `setCredential` from `src/libs/commands`. Keep the dialog open if Provider creation itself fails.

- [ ] **Step 4: Orchestrate deletion and selection cleanup.**

Handle the list’s delete intent with the existing `useDialog` confirmation. Call `providerStore.deleteProvider(provider.name)`. If the deleted Provider was selected, clear the selected name after deletion; otherwise preserve the current selection.

- [ ] **Step 5: Add scoped styling.**

Use theme variables for body/card/background colors, 16px page spacing consistent with SkillsView, a minimum left panel width that supports Provider metadata, independent `overflow: auto` panel regions, and a responsive fallback where the split stacks or narrows at small widths. Add `prefers-reduced-motion: reduce` to disable non-essential transitions.

- [ ] **Step 6: Run the full frontend validation.**

Run: `npm run build && npm test`

Expected: TypeScript/Vue compilation passes and all existing plus new frontend tests pass.

---

## Task 7: Review, diagnostics, and regression validation

**Files:**
- Inspect all modified frontend files from Tasks 1–6.

- [ ] **Step 1: Run project diagnostics.**

Run the project diagnostics tool for the changed files, then resolve diagnostics introduced by this redesign. Do not delete meaningful code to silence unrelated pre-existing diagnostics.

- [ ] **Step 2: Run Rust checks without touching migration changes.**

Run:

```sh
cargo test -p wisp-configs
cargo test -p wisp-llm
```

Expected: report pass/fail separately from frontend checks. If a failure belongs to the pre-existing rig migration workspace, document it rather than reverting that work.

- [ ] **Step 3: Verify the final behavioral contract manually.**

Check the running UI for:

1. Add provider hides the internal ID input and shows a generated preview.
2. Duplicate Display names generate `-2`, `-3` IDs.
3. Changing Display name in the details form does not change the ID shown in the identity section.
4. API key remains masked and is saved through the keyring path.
5. Fetch models adds only previously unseen `metadata.name` values.
6. Existing model display names, parameters, capabilities, and metadata remain unchanged after sync.
7. Empty, loading, success, warning, and error states are visible.
8. Icon-only actions expose tooltip/ARIA labels and keyboard focus.

- [ ] **Step 4: Review the diff for scope.**

Run:

```sh
git --no-pager diff -- src/views/ProvidersView.vue src/components/ProviderList.vue src/components/ProviderConfig.vue src/components/ProviderDetailForm.vue src/components/ModelTable.vue src/components/AddProviderDialog.vue src/stores/provider.ts src/utils/provider.ts src/__tests__/utils/provider.test.ts
```

Confirm no Skills files, backend commands, dependency manifests, or unrelated migration files were changed.

---

## Completion Criteria

The work is ready for review when:

- `ProvidersView.vue` has a SkillsView-aligned header and two-column master-detail layout.
- Provider creation accepts Display name rather than a user-entered ID.
- Generated IDs are sanitized, unique, and stable after creation.
- API keys remain separate from Provider serialization and failures are reported independently.
- Model synchronization only appends remote models with new `metadata.name` values.
- Current discriminated `ModelInfo` and new model metadata fields are rendered safely.
- Focused tests, frontend build/tests, and applicable Rust checks have been run with results reported.
