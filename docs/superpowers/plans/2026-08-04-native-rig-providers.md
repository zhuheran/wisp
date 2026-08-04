# Native Rig Provider Registry Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Route each supported API-key chat Provider through its native `rig-core` adapter and expose every supported ProviderKind in the Provider UI.

**Architecture:** Expand `wisp-configs::provider::ApiType` into the native chat ProviderKind enum while keeping its `api_type` serialized field for existing TOML compatibility. `wisp-llm` dispatches concrete native rig clients to a generic CompletionModel streaming helper; native model listing is dispatched only for kinds that support it. The frontend uses a descriptor registry to drive labels, Base URL requirements, and model-discovery controls.

**Tech Stack:** Rust 2021, rig-core 0.41.0, Tauri 2, Vue 3, TypeScript, Pinia, Naive UI, Vitest.

## Global Constraints

- Include native rig providers that support chat completion with API-key or local configuration; exclude OAuth and non-chat-only providers.
- Keep `open_ai_compatible` as the required-custom-endpoint fallback.
- Never serialize API keys; resolve them through KeyManager using the stable Provider ID.
- Keep the serialized Provider field named `api_type` for legacy `configs.toml` compatibility.
- DeepSeek completion and model listing must use `rig_core::providers::deepseek::Client`.
- Never call `.base_url("")` for a native provider; only apply a URL override when non-empty.
- Model fetch only exists for provider kinds that rig natively supports through `ModelListingClient`.
- Preserve existing Provider IDs and model configuration during migration.

---

### Task 1: Expand ProviderKind and add capability helpers

**Files:**
- Modify: `crates/wisp-configs/src/provider.rs`
- Test: inline Rust tests in `crates/wisp-configs/src/provider.rs`

**Produces:**

```rust
pub enum ApiType {
  OpenAi, DeepSeek, Anthropic, Cohere, Gemini, Groq, HuggingFace,
  Hyperbolic, Llamafile, MiniMax, Mira, Mistral, Moonshot, Ollama,
  OpenRouter, Perplexity, Together, XAi, XiaomiMiMo, ZAi, Azure, Doubleword, OpenAiCompatible,
}

impl ApiType {
  pub const ALL: &'static [Self];
  pub fn supports_model_listing(&self) -> bool;
  pub fn allows_custom_base_url(&self) -> bool;
  pub fn requires_base_url(&self) -> bool;
}
```

- [ ] Write failing tests that assert legacy values deserialize, all enum variants have unique snake-case serialization, OpenAI Compatible requires a URL, native providers do not require URLs, and native listing flags are correct for OpenAI/DeepSeek/Anthropic/Gemini/Mistral/OpenRouter/Ollama/XiaomiMiMo.
- [ ] Run `cargo test -p wisp-configs provider` and confirm the tests fail before helpers exist.
- [ ] Implement the expanded enum and pure helpers without changing Provider IDs or KeyManager methods.
- [ ] Run `cargo test -p wisp-configs provider` and confirm it passes.

### Task 2: Implement generic native-rig completion routing

**Files:**
- Modify: `crates/wisp-llm/src/lib.rs`
- Test: inline Rust tests in `crates/wisp-llm/src/lib.rs`

**Produces:**

```rust
async fn stream_model<M: CompletionModel>(... model: M ...) -> Result<StreamOutcome, LlmError>
pub async fn stream(...) -> Result<StreamOutcome, LlmError>
```

- [ ] Write failing unit tests for `ApiType::DeepSeek` route selection and URL override policy using a pure `routing_kind`/`should_override_base_url` helper exposed only inside the module.
- [ ] Run `cargo test -p wisp-llm routing` and confirm failure.
- [ ] Replace the fixed OpenAI `CompletionsClient` constructor with a `match provider.api_type` that constructs each matching rig client. Apply `.base_url` only where the kind allows it and the configured URL is non-empty.
- [ ] Use the generic helper for request conversion, stream draining, cancellation, and accumulated tool calls so all branches retain identical app behavior.
- [ ] DeepSeek branch must construct `rig_core::providers::deepseek::Client`; `stream_model` must use its native CompletionModel.
- [ ] Run `cargo test -p wisp-llm` and confirm all tests pass.

### Task 3: Route model listing by native provider capability

**Files:**
- Modify: `crates/wisp-llm/src/lib.rs`
- Modify: `src-tauri/src/provider_commands.rs`
- Test: inline Rust tests in both changed files

**Produces:**

```rust
pub async fn list_models(provider: &Provider) -> Result<rig_core::model::ModelList, LlmError>
```

- [ ] Write failing tests proving DeepSeek listing selects the DeepSeek native route and unsupported listing returns an error naming the provider kind.
- [ ] Run focused `cargo test -p wisp-llm model_listing` and confirm failure.
- [ ] Add native listing branches only for kinds whose rig Capability is `ModelListing = Capable`: OpenAI, DeepSeek, Anthropic, Gemini, Mistral, OpenRouter, Ollama, XiaomiMiMo. Azure and Doubleword remain manual-model providers because rig declares no native ModelListing capability for them. Keep OpenAI Compatible on the OpenAI listing client with its required custom URL.
- [ ] Change Tauri `provider_fetch_models` to call `wisp_llm::list_models`, retaining the existing `to_wisp_model` mapping and no-key-over-Tauri guarantee.
- [ ] Run `cargo test -p wisp-llm && cargo test -p wisp-configs`.

### Task 4: Add frontend provider descriptor registry and types

**Files:**
- Modify: `src/libs/types.ts`
- Create: `src/libs/provider-descriptors.ts`
- Create: `src/__tests__/libs/provider-descriptors.test.ts`

**Produces:**

```ts
export type ApiType = /* all serialized native ProviderKind values */
export interface ProviderDescriptor {
  value: ApiType
  label: string
  group: 'Hosted' | 'Local' | 'Compatible'
  allowsCustomBaseUrl: boolean
  requiresBaseUrl: boolean
  supportsModelListing: boolean
  requiresApiKey: boolean
}
export const providerDescriptors: readonly ProviderDescriptor[]
export function providerDescriptor(type?: ApiType): ProviderDescriptor
```

- [ ] Write failing Vitest tests for unique values, complete expected provider set, Base URL behavior, and native listing flags.
- [ ] Run `npm test -- src/__tests__/libs/provider-descriptors.test.ts` and confirm failure.
- [ ] Implement the type and descriptor table matching Task 1's serialized values.
- [ ] Run the focused tests and confirm pass.

### Task 5: Make Provider forms descriptor-driven

**Files:**
- Modify: `src/components/AddProviderDialog.vue`
- Modify: `src/components/ProviderDetailForm.vue`
- Modify: `src/components/ProviderList.vue`
- Modify: `src/components/ProviderConfig.vue`

- [ ] Replace hardcoded three-item API-type lists with grouped descriptor options.
- [ ] Use descriptor flags to show Base URL only when permitted and require it only for OpenAI Compatible; do not clear optional local endpoints for Ollama/Llamafile.
- [ ] Render labels from descriptors in list/detail identity.
- [ ] Keep API key hidden only for `llamafile`; keep all other current-scope providers on existing KeyManager flow.
- [ ] Run `npm run build`.

### Task 6: Gate model discovery in ModelTable

**Files:**
- Modify: `src/components/ModelTable.vue`

- [ ] Use `providerDescriptor(provider.api_type).supportsModelListing` to hide/disable Fetch models and show a short manual-configuration explanation when unavailable.
- [ ] Preserve append-only sync and manually adding/editing/deleting models for every ProviderKind.
- [ ] Run `npm test && npm run build`.

### Task 7: Full regression verification

- [ ] Run `cargo test -p wisp-configs`.
- [ ] Run `cargo test -p wisp-llm`.
- [ ] Run `npm run build`.
- [ ] Run `npm test`.
- [ ] Run diagnostics on all modified Rust/TypeScript/Vue files and fix issues introduced by the change.
- [ ] Inspect `git diff --check` and confirm no OAuth, non-chat provider, or unrelated migration files are added or reverted.
