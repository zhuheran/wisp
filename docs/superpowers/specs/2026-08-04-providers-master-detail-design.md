# Provider Management Master-Detail Redesign

**Date:** 2026-08-04  
**Status:** Approved design; implementation pending  
**Scope:** Frontend Provider management refactor. Do not modify Skills or add backend commands.

## Goals

1. Modernize Provider management around a page-level master-detail workspace.
2. Use `SkillsView.vue` as the visual baseline: page header, action placement, spacing, theme-token usage, loading and empty states.
3. Keep Provider editing aligned with the migrated Rust configuration contract.
4. Remove manual entry of the Provider internal ID.
5. Make model synchronization safe: only add remote models that do not yet exist locally.
6. Preserve existing Tauri commands and the separate Keyring flow for API keys.

## Non-goals

- Do not change `SkillsView.vue` or add a Skill-content panel in this phase.
- Do not add Provider connection testing, sync history, model filtering, or a third workspace column.
- Do not add new backend commands.
- Do not overwrite or roll back the in-progress rig migration changes in the working tree.
- Do not replace the existing manual Model editor.

## Confirmed Data Contract

### Provider

The frontend consumes the serialized `wisp_configs::provider::Provider` shape:

```ts
interface Provider {
  name: string
  display_name: string
  base_url: string
  models: Model[]
  api_type?: 'open_ai' | 'deep_seek' | 'open_ai_compatible'
}
```

- `name` is a stable, internal Provider ID.
- The keyring stores API keys under `provider.name`.
- `display_name` is user-facing and can change after creation.
- Updating a Provider serializes the full Provider configuration through `configs_update_provider`.

### API keys

API keys do not belong to the Provider serialized data.

```ts
getCredential(provider.name)
setCredential(provider.name, apiKey)
```

The provider configuration and API key remain separate write paths. A failed keyring write after successful Provider creation or update must be reported independently and must not misrepresent the Provider configuration result.

### Model

```ts
interface Model {
  metadata: ModelMetadata
  model_info: ModelInfo
}

interface ModelMetadata {
  name: string
  display_name: string
  description?: string
  context_window?: number
  owned_by?: string
}
```

`ModelInfo` is a discriminated union serialized by Rust as `type` plus `configs`:

```ts
type ModelInfo =
  | {
      type: 'text_generation'
      configs: {
        parameters: TextGenerationParams
        capabilities: TextModelCapability[]
        multimodal?: MultimodalConfig
      }
    }
  | { type: 'image_generation'; configs: { parameters: ImageGenerationParams } }
  | { type: 'embedding'; configs: { parameters: EmbeddingParams } }
  | { type: 'reranker'; configs: { parameters: RerankerParams } }
  | { type: 'audio' }
```

`context_window` is model metadata, not a sampling parameter. `owned_by` is provider-listing metadata. Access to `configs.capabilities` must be narrowed to the `text_generation` branch.

## Page Design

### Page-level layout

`ProvidersView.vue` becomes the workspace owner:

```text
Providers                                      Add provider
Manage API providers and available models

┌──────────────────────┬───────────────────────────────────────┐
│ Providers            │ Selected provider                     │
│ configured count     │ Provider identity                      │
│                      │ Settings                               │
│ Provider list        │ Models                                 │
└──────────────────────┴───────────────────────────────────────┘
```

- Header follows the `SkillsView.vue` pattern:
  - title: `Providers`
  - subtitle: `Manage API providers and available models`
  - primary action: `Add provider`
- Below the header, use a two-column master-detail workspace.
- Keep a resizable divider through Naive UI `n-split`; make the left navigation wide enough to display metadata consistently.
- Each column owns its scrolling region; the page header remains visible.
- Use existing Naive UI theme variables rather than raw color values.
- Transitions stay subtle (150–300 ms) and preserve visible keyboard focus.

### Provider list

`ProviderList.vue` becomes navigation only.

It owns:

- Provider selection.
- Displaying `display_name`, API type, and model count.
- Clear selected state.
- Keyboard activation.
- Delete action with confirmation.

It does not own:

- Provider creation state.
- The add-provider modal.
- Provider configuration editing.

The parent passes the selected Provider ID and receives `select` / `delete` intent events.

### Provider detail workspace

`ProviderConfig.vue` owns layout composition for a selected Provider:

1. **Identity area**
   - `display_name` as the heading.
   - API-type label and model count as supporting metadata.
   - internal ID shown as low-emphasis, read-only metadata (`ID: openai`).

2. **Provider settings**
   - existing `ProviderDetailForm.vue`, with a clearer section heading.
   - editable fields: display name, base URL, API type, API key.
   - `name` is never an editable input.

3. **Models**
   - existing `ModelTable.vue`, with model synchronization and manual management controls.

The selected-provider empty state tells users to select a Provider. With no Providers, it directs the user to the page-level Add Provider action.

## Add Provider Flow

### Component boundary

Create `AddProviderDialog.vue`. It is controlled by `ProvidersView.vue` and emits a ready-to-create payload plus optional API key. It should not write directly to the store.

### User fields

- Display name (required)
- API type (required; defaults to `open_ai_compatible`)
- Base URL
- API key (optional)

The internal ID has a read-only preview and is not a user-editable input.

### Provider ID generation

Generate the Provider `name` client-side from `display_name`:

1. trim whitespace;
2. lowercase;
3. replace runs of non-ASCII letters or digits with `-`;
4. collapse and trim `-` separators;
5. use `provider` if the result is empty;
6. resolve collisions against loaded Provider IDs by appending `-2`, `-3`, and so on.

Examples:

```text
OpenAI              -> openai
DeepSeek Official   -> deepseek-official
OpenAI Compatible   -> openai-compatible
```

For a display name containing no supported ID characters, the preview falls back to `provider`, then uses suffix collision resolution as needed.

### Creation sequence

1. Validate required fields locally and show form errors near their fields.
2. Construct the full Provider payload with `models: []`.
3. Call `providerStore.createProvider(provider)`.
4. If an API key was supplied, call `setCredential(provider.name, apiKey)`.
5. Reload the Provider list and select the newly created Provider.
6. Close the dialog only when the Provider creation succeeds.

If Provider creation succeeds but API key saving fails, keep the Provider, select it, and report that the API key needs to be saved from the detail form.

## Provider Editing

- The Provider ID never changes after creation.
- Changing `display_name` does not rename the internal ID or keyring key.
- Existing edit/cancel behavior remains: a form clone is edited, saved through the store, or reset from props.
- Save feedback must distinguish configuration save failures from API-key save failures.
- Buttons must show loading feedback and accessible labels/tooltips when icon-only.

## Model Synchronization

### Backend contract

`provider_fetch_models(provider.name)` calls the rig-backed backend listing. Returned models include rig listing metadata mapped by `to_wisp_model`:

- `metadata.name`
- `metadata.display_name`
- optional `metadata.description`
- optional `metadata.context_window`
- optional `metadata.owned_by`
- `model_info` inferred from the model ID, including type and text capabilities.

### Merge rule

Sync only adds remote models whose `metadata.name` does not already exist in the Provider configuration.

```ts
const existingNames = new Set(
  provider.models.map((model) => model.metadata.name),
)

const newModels = fetched.filter(
  (model) => !existingNames.has(model.metadata.name),
)

const merged = [...provider.models, ...newModels]
```

This is intentional:

- Existing local Models remain fully unchanged.
- A sync never overwrites local display names, descriptions, parameter defaults, capabilities, multimodal settings, or metadata.
- New Models retain backend-provided listing metadata and inferred ModelInfo.
- If there are no new Models, do not write the Provider configuration.

### UI feedback

- Request in progress: disable/reload-control loading state.
- New models: `Added N new model(s)`.
- No new models: neutral informational message such as `All fetched models are already configured`.
- Request failure: retain current table state and display the backend error.

### Model table presentation

Continue to support add, edit, and delete. Improve display by prioritizing:

- display name and model ID;
- `model_info.type`;
- text-generation capabilities only;
- `metadata.owned_by`;
- `metadata.context_window`;
- description in a compact, optional secondary treatment.

Do not treat `context_window` as a sampling parameter. Use narrowed branches/type guards for discriminated `ModelInfo` access; do not introduce new broad unsafe casts during the refactor.

## Accessibility and Interaction

- Use visible textual labels for form fields; do not use placeholders as labels.
- Keep logical Tab order aligned with layout.
- Preserve visible keyboard focus.
- Add `aria-label` and Naive UI tooltip text to icon-only controls (edit, delete, fetch, add model).
- Use loading states for asynchronous actions.
- Use confirmation for destructive Provider and Model deletion.
- Avoid hover-only actions.

## File Scope

Expected frontend work:

- `src/views/ProvidersView.vue`
- `src/components/ProviderList.vue`
- `src/components/ProviderConfig.vue`
- `src/components/ProviderDetailForm.vue`
- `src/components/ModelTable.vue`
- `src/components/AddProviderDialog.vue` (new)
- `src/stores/provider.ts`
- focused frontend tests for pure ID generation and synchronization behavior, if test conventions support them

Do not edit Skills files. Do not add new backend commands.

## Validation

Run focused checks first, then project checks:

```sh
npm run build
npm test
cargo test -p wisp-configs
cargo test -p wisp-llm
```

Also inspect TypeScript narrowing in the updated model UI. Preserve the existing uncommitted rig migration work and report unrelated validation failures without reverting them.
