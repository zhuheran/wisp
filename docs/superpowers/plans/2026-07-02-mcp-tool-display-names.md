# MCP Tool Display Names Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Generate human-friendly LLM display names (`{ServerName} {Verb} {Noun}`) for MCP tools, backed by a reusable Chore LLM setting.

**Architecture:** A user-configured Chore LLM (`(provider, model)` reference, key resolved via existing `KeyManager`) drives a Rust batched completion. Display names are cached in `localStorage` keyed by a hash of the tool `description` and attached to `RegisteredTool` at runtime. A new `/settings` view configures the Chore LLM.

**Tech Stack:** Rust (Tauri, `async_openai` 0.28.2, reqwest), Vue 3 + Pinia + naive-ui, vitest, cargo.

## Global Constraints

- Chore LLM stores **no secret**; key resolved at call time via `KeyManager::new("wisp".to_string()).get_api_key(&provider.name)`.
- Display-name format: `{ServerName} {Verb} {Noun}`, title-cased; noun may be multiple words. Server name = the MCP server's `name`.
- Cache key = hash of tool `description` (synchronous FNV-1a, works in browser + node test env).
- Display-name generation must **never block** tool loading or throw to the UI; failures fall back to raw `name`.
- Follow existing patterns: config get/set like `default_responder_id`; LLM client like `src-tauri/src/api.rs`; Tauri command like `configs_get_default_responder`.
- No code comments unless requested. Library crate name is `wisp_lib`.

---

### Task 1: Backend — ChoreLlmRef type + ConfigManager get/set

**Files:**
- Modify: `src-tauri/src/configs/mod.rs`

**Interfaces:**
- Produces: `configs::ChoreLlmRef { provider: String, model: String }`, `ConfigManager::get_chore_llm() -> Option<ChoreLlmRef>`, `ConfigManager::set_chore_llm(Option<ChoreLlmRef>) -> Result<(), ConfigError>`.

- [ ] **Step 1: Add the ChoreLlmRef struct and Config field**

In `src-tauri/src/configs/mod.rs`, add the struct near the top (after the `use` block, before `struct Config`):

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChoreLlmRef {
    pub provider: String,
    pub model: String,
}
```

Add the field to `Config` (alongside `default_responder_id`):

```rust
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
struct Config {
    providers: Vec<provider::Provider>,
    characters: Vec<character::Character>,
    #[serde(default)]
    default_responder_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    chore_llm: Option<ChoreLlmRef>,
}
```

- [ ] **Step 2: Add get/set methods to ConfigManager**

Add at the end of the `impl ConfigManager` block (after `set_default_responder`):

```rust
    // ========== Chore LLM ==========

    /// Get the chore LLM reference.
    pub fn get_chore_llm(&self) -> Option<ChoreLlmRef> {
        self.configs.lock().unwrap().chore_llm.clone()
    }

    /// Set the chore LLM reference.
    pub fn set_chore_llm(&self, chore_llm: Option<ChoreLlmRef>) -> Result<(), ConfigError> {
        let mut configs = self.configs.lock().unwrap();
        configs.chore_llm = chore_llm;
        std::mem::drop(configs);
        self.save()?;
        Ok(())
    }
```

- [ ] **Step 3: Verify it compiles**

Run: `cargo check --manifest-path src-tauri/Cargo.toml`
Expected: compiles with no errors.

- [ ] **Step 4: Commit**

```bash
git add src-tauri/src/configs/mod.rs
git commit -m "feat(config): add chore_llm (provider, model) ref to ConfigManager"
```

---

### Task 2: Backend — chore_llm config commands

**Files:**
- Modify: `src-tauri/src/commands.rs`
- Modify: `src-tauri/src/lib.rs`

**Interfaces:**
- Consumes: `crate::configs::ChoreLlmRef`, `ConfigManager::get_chore_llm`/`set_chore_llm` (Task 1).
- Produces: Tauri commands `configs_get_chore_llm`, `configs_set_chore_llm`.

- [ ] **Step 1: Add the two commands**

At the end of `src-tauri/src/commands.rs` (after `configs_set_default_responder`):

```rust
#[tauri::command]
pub async fn configs_get_chore_llm(
    app_handle: AppHandle,
) -> Result<Option<crate::configs::ChoreLlmRef>, String> {
    let state = app_handle.state::<Mutex<AppData>>();
    let state = state.lock().unwrap();
    Ok(state.config_manager.get_chore_llm())
}

#[tauri::command]
pub async fn configs_set_chore_llm(
    app_handle: AppHandle,
    chore_llm: Option<crate::configs::ChoreLlmRef>,
) -> Result<(), String> {
    let state = app_handle.state::<Mutex<AppData>>();
    let state = state.lock().unwrap();
    state
        .config_manager
        .set_chore_llm(chore_llm)
        .map_err(|e| e.to_string())
}
```

Confirm `ChoreLlmRef` is exported from the `configs` module: it is `pub struct` in `configs/mod.rs`, and the module is declared `pub mod` — verify `src-tauri/src/configs/mod.rs` re-exports it. It's already public at `crate::configs::ChoreLlmRef`. (No `pub use` needed since the struct is defined directly in `mod.rs`.)

- [ ] **Step 2: Register both commands**

In `src-tauri/src/lib.rs`, inside the `invoke_handler(tauri::generate_handler![ ... ])` list, add (next to the other `configs_*` entries, after `configs_set_default_responder`):

```rust
			commands::configs_get_chore_llm,
			commands::configs_set_chore_llm,
```

- [ ] **Step 3: Verify it compiles**

Run: `cargo check --manifest-path src-tauri/Cargo.toml`
Expected: compiles with no errors.

- [ ] **Step 4: Commit**

```bash
git add src-tauri/src/commands.rs src-tauri/src/lib.rs
git commit -m "feat(commands): add configs_get/set_chore_llm commands"
```

---

### Task 3: Backend — chore module: parser (TDD), chore_complete, generate command

**Files:**
- Create: `src-tauri/src/chore.rs`
- Modify: `src-tauri/src/lib.rs` (declare module + register command)

**Interfaces:**
- Consumes: `crate::configs::ChoreLlmRef`, `ConfigManager`, `KeyManager`, `AppData`.
- Produces: pure fn `parse_display_names(raw: &str) -> HashMap<String, String>`; async fn `chore_complete(app_handle, system, user) -> Result<String, String>`; command `mcp_generate_tool_display_names(app_handle, tools) -> Result<HashMap<String,String>, String>`.

- [ ] **Step 1: Write the failing test for the parser**

Create `src-tauri/src/chore.rs` with only the test first:

```rust
#[cfg(test)]
mod tests {
    use super::parse_display_names;
    use std::collections::HashMap;

    #[test]
    fn parses_well_formed_array() {
        let raw = r#"[
            {"name":"read_file","display_name":"Filesystem Read File"},
            {"name":"create_issue","display_name":"Github Create Issue"}
        ]"#;
        let mut expected = HashMap::new();
        expected.insert("read_file".to_string(), "Filesystem Read File".to_string());
        expected.insert("create_issue".to_string(), "Github Create Issue".to_string());
        assert_eq!(parse_display_names(raw), expected);
    }

    #[test]
    fn strips_markdown_code_fences() {
        let raw = "```json\n[{\"name\":\"x\",\"display_name\":\"X Do Thing\"}]\n```";
        let map = parse_display_names(raw);
        assert_eq!(map.get("x"), Some(&"X Do Thing".to_string()));
    }

    #[test]
    fn returns_empty_for_malformed() {
        assert!(parse_display_names("not json at all").is_empty());
        assert!(parse_display_names("[{").is_empty());
    }

    #[test]
    fn skips_invalid_entries_keeps_valid() {
        let raw = r#"[
            {"name":"good","display_name":"Good Do Thing"},
            {"name":"bad"},
            {"display_name":"No Name"}
        ]"#;
        let map = parse_display_names(raw);
        assert_eq!(map.len(), 1);
        assert_eq!(map.get("good"), Some(&"Good Do Thing".to_string()));
    }

    #[test]
    fn accepts_camel_case_key() {
        let raw = r#"[{"name":"x","displayName":"X Do Thing"}]"#;
        let map = parse_display_names(raw);
        assert_eq!(map.get("x"), Some(&"X Do Thing".to_string()));
    }
}
```

- [ ] **Step 2: Run the test to verify it fails**

Run: `cargo test --manifest-path src-tauri/Cargo.toml chore::`
Expected: FAIL — `cannot find function parse_display_names` (compile error).

- [ ] **Step 3: Implement the parser (minimal)**

Add above the `tests` module in `src-tauri/src/chore.rs`:

```rust
use serde_json::Value;
use std::collections::HashMap;

pub fn parse_display_names(raw: &str) -> HashMap<String, String> {
    let trimmed = raw
        .trim()
        .trim_start_matches("```json")
        .trim_start_matches("```")
        .trim_end_matches("```")
        .trim();

    let parsed: Value = match serde_json::from_str(trimmed) {
        Ok(v) => v,
        Err(_) => return HashMap::new(),
    };

    let arr = match parsed.as_array() {
        Some(a) => a,
        None => return HashMap::new(),
    };

    let mut out = HashMap::new();
    for entry in arr {
        let name = entry.get("name").and_then(|v| v.as_str());
        let display = entry
            .get("display_name")
            .or_else(|| entry.get("displayName"))
            .and_then(|v| v.as_str());
        if let (Some(name), Some(display)) = (name, display) {
            if !name.is_empty() && !display.is_empty() {
                out.insert(name.to_string(), display.trim().to_string());
            }
        }
    }
    out
}
```

- [ ] **Step 4: Run the test to verify it passes**

Run: `cargo test --manifest-path src-tauri/Cargo.toml chore::`
Expected: PASS — 5 tests.

- [ ] **Step 5: Implement chore_complete + generate command**

Add to `src-tauri/src/chore.rs` (keep `parse_display_names` and `tests` as-is; add imports + functions):

Top of file, replace the `use` block with:

```rust
use async_openai::{
    config::OpenAIConfig,
    types::{
        ChatCompletionRequestMessage, CreateChatCompletionRequestArgs,
    },
    Client,
};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::HashMap;
use tauri::{AppHandle, Manager, Runtime};
```

Then add (after `parse_display_names`):

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ToolDisplayNameInput {
    pub server_name: String,
    pub tool_name: String,
    pub description: Option<String>,
}
```

/// Resolve the configured chore LLM and run a non-streaming completion.
/// Returns `Err` if no chore LLM is configured or the call fails.
pub async fn chore_complete<R: Runtime>(
    app_handle: &AppHandle<R>,
    system: &str,
    user: &str,
) -> Result<String, String> {
    use std::sync::Mutex;
    use crate::types::AppData;
    use crate::key_manager::KeyManager;

    let (provider, model, base_url) = {
        let state = app_handle.state::<Mutex<AppData>>();
        let state = state.lock().map_err(|e| e.to_string())?;
        let chore = state
            .config_manager
            .get_chore_llm()
            .ok_or_else(|| "Chore LLM not configured".to_string())?;
        let provider = state
            .config_manager
            .get_provider(&chore.provider)
            .ok_or_else(|| format!("Provider '{}' not found", chore.provider))?;
        (provider, chore.model, provider.base_url)
    };

    let key_manager = KeyManager::new("wisp".to_string());
    let api_key = key_manager
        .get_api_key(&provider.name)
        .or_else(|_| std::env::var("OPENAI_API_KEY"))
        .map_err(|e| format!("Failed to resolve API key: {e}"))?;

    let config = OpenAIConfig::new()
        .with_api_base(base_url)
        .with_api_key(api_key);
    let client = Client::with_config(config);

    let messages: Vec<ChatCompletionRequestMessage> = vec![
        serde_json::from_value(serde_json::json!({
            "role": "system", "content": system
        }))
        .map_err(|e| format!("message build error: {e}"))?,
        serde_json::from_value(serde_json::json!({
            "role": "user", "content": user
        }))
        .map_err(|e| format!("message build error: {e}"))?,
    ];

    let request = CreateChatCompletionRequestArgs::default()
        .model(model.clone())
        .messages(messages)
        .temperature(0.0)
        .max_tokens(2048_u32)
        .build()
        .map_err(|e| format!("request build error: {e}"))?;

    let response = client
        .chat()
        .create(request)
        .await
        .map_err(|e| format!("chore completion failed for model '{model}': {e}"))?;

    let text = response
        .choices
        .first()
        .and_then(|c| c.message.content.clone())
        .unwrap_or_default();
    Ok(text)
}

const DISPLAY_NAME_SYSTEM: &str = "You generate concise, human-friendly display names for MCP tools. Reply ONLY with a JSON array, no prose. Each element: {\"name\": <original tool name>, \"display_name\": <string>}. The display_name MUST follow the structure: `<ServerName> <Verb> <Noun>` where ServerName is the provided server name (one word, Title Case), Verb is one word (Title Case), and Noun may be one or more words (Title Case). Example inputs server='filesystem', name='read_file' -> 'Filesystem Read File'.";

const DISPLAY_NAME_USER_TEMPLATE: &str = "Generate a display name for each tool. Tools:\n";

/// Generate display names for a batch of tools using the chore LLM.
/// Returns a map of tool_name -> display_name. Never fails the whole batch:
/// unparseable/missing entries are simply absent from the result.
#[tauri::command]
pub async fn mcp_generate_tool_display_names(
    app_handle: AppHandle,
    tools: Vec<ToolDisplayNameInput>,
) -> Result<HashMap<String, String>, String> {
    if tools.is_empty() {
        return Ok(HashMap::new());
    }

    let payload: Vec<Value> = tools
        .iter()
        .map(|t| {
            serde_json::json!({
                "server": t.server_name,
                "name": t.tool_name,
                "description": t.description.clone().unwrap_or_default(),
            })
        })
        .collect();

    let user = format!(
        "{}{}",
        DISPLAY_NAME_USER_TEMPLATE,
        serde_json::to_string_pretty(&payload).unwrap_or_default()
    );

    let raw = match chore_complete(&app_handle, DISPLAY_NAME_SYSTEM, &user).await {
        Ok(text) => text,
        Err(e) => {
            eprintln!("[chore] display-name generation failed: {e}");
            return Ok(HashMap::new());
        }
    };

    Ok(parse_display_names(&raw))
}
```

- [ ] **Step 6: Declare the module and register the command**

In `src-tauri/src/lib.rs`, add to the `mod` declarations (next to `mod tool_registry;`):

```rust
mod chore;
```

In the `invoke_handler` list (with the other registry commands, after `tool_registry::registry_refresh,`):

```rust
			chore::mcp_generate_tool_display_names,
```

- [ ] **Step 7: Verify it compiles and tests pass**

Run: `cargo test --manifest-path src-tauri/Cargo.toml`
Expected: all tests PASS (incl. the 5 `chore::` tests), compiles cleanly.

- [ ] **Step 8: Commit**

```bash
git add src-tauri/src/chore.rs src-tauri/src/lib.rs
git commit -m "feat(chore): add chore_complete helper + display-name generation command"
```

---

### Task 4: Frontend — types + command wrappers

**Files:**
- Modify: `src/libs/types.ts`
- Modify: `src/libs/commands.ts`

**Interfaces:**
- Consumes: backend commands `configs_get_chore_llm`, `configs_set_chore_llm`, `mcp_generate_tool_display_names`.
- Produces: `RegisteredTool.displayName`, `mcpGetChoreLlm()`, `mcpSetChoreLlm()`, `mcpGenerateToolDisplayNames()`.

- [ ] **Step 1: Add `displayName` to RegisteredTool**

In `src/libs/types.ts`, add to `RegisteredTool` (after `originalName?: string;`):

```ts
	displayName?: string;
```

- [ ] **Step 2: Add command wrappers**

At the end of `src/libs/commands.ts`:

```ts
export interface ChoreLlmRef {
	provider: string;
	model: string;
}

export interface ToolDisplayNameInput {
	serverName: string;
	toolName: string;
	description?: string;
}

export async function mcpGetChoreLlm() {
	return invoke<ChoreLlmRef | null>('configs_get_chore_llm', {})
}

export async function mcpSetChoreLlm(choreLlm: ChoreLlmRef | null) {
	return invoke<void>('configs_set_chore_llm', { choreLlm })
}

export async function mcpGenerateToolDisplayNames(tools: ToolDisplayNameInput[]) {
	return invoke<Record<string, string>>('mcp_generate_tool_display_names', { tools })
}
```

- [ ] **Step 3: Verify types**

Run: `npx vue-tsc --noEmit`
Expected: no NEW errors in `commands.ts` or `types.ts` (pre-existing unrelated errors are fine).

- [ ] **Step 4: Commit**

```bash
git add src/libs/types.ts src/libs/commands.ts
git commit -m "feat(frontend): add chore llm + display-name command wrappers"
```

---

### Task 5: Frontend — useChoreLlm composable

**Files:**
- Create: `src/composables/useChoreLlm.ts`

**Interfaces:**
- Consumes: `mcpGetChoreLlm`, `mcpSetChoreLlm` (Task 4), `useProviderStore`.
- Produces: `useChoreLlm()` -> `{ choreLlm, loading, providerOptions, modelOptions, save, clear }`.

- [ ] **Step 1: Create the composable**

```ts
import { ref, computed } from 'vue'
import { useProviderStore } from '../stores/provider'
import { mcpGetChoreLlm, mcpSetChoreLlm, type ChoreLlmRef } from '../libs/commands'

export function useChoreLlm() {
  const providerStore = useProviderStore()
  const choreLlm = ref<ChoreLlmRef | null>(null)
  const loading = ref(false)

  const providerOptions = computed(() =>
    providerStore.providers.map((p) => ({ label: p.display_name || p.name, value: p.name }))
  )

  const modelOptions = computed(() => {
    const providerName = choreLlm.value?.provider
    if (!providerName) return []
    const provider = providerStore.providers.find((p) => p.name === providerName)
    if (!provider) return []
    return provider.models
      .filter((m) => m.model_info.type === 'text_generation')
      .map((m) => ({ label: m.metadata.display_name || m.metadata.name, value: m.metadata.name }))
  })

  const load = async () => {
    loading.value = true
    try {
      choreLlm.value = await mcpGetChoreLlm()
    } finally {
      loading.value = false
    }
  }

  const save = async () => {
    await mcpSetChoreLlm(choreLlm.value)
  }

  const clear = async () => {
    choreLlm.value = null
    await mcpSetChoreLlm(null)
  }

  load()

  return { choreLlm, loading, providerOptions, modelOptions, save, clear }
}
```

- [ ] **Step 2: Verify types**

Run: `npx vue-tsc --noEmit`
Expected: no NEW errors in `useChoreLlm.ts`.

- [ ] **Step 3: Commit**

```bash
git add src/composables/useChoreLlm.ts
git commit -m "feat(frontend): add useChoreLlm composable"
```

---

### Task 6: Frontend — Settings view + route + sidebar entry

**Files:**
- Create: `src/views/SettingsView.vue`
- Modify: `src/router/index.ts`
- Modify: `src/App.vue`

**Interfaces:**
- Consumes: `useChoreLlm` (Task 5), naive-ui components, `@vicons/fluent` `Settings24Regular`.

- [ ] **Step 1: Create the SettingsView**

```vue
<script setup lang="ts">
import { computed } from 'vue'
import { NCard, NForm, NFormItem, NSelect, NButton, NSpace, useMessage } from 'naive-ui'
import { useChoreLlm } from '../composables/useChoreLlm'

const message = useMessage()
const { choreLlm, providerOptions, modelOptions, save, clear } = useChoreLlm()

const selectedProvider = computed<string | null>({
  get: () => choreLlm.value?.provider ?? null,
  set: (val) => {
    if (val) {
      choreLlm.value = { provider: val, model: '' }
    } else {
      choreLlm.value = null
    }
  },
})

const selectedModel = computed<string | null>({
  get: () => choreLlm.value?.model ?? null,
  set: (val) => {
    if (choreLlm.value && val) {
      choreLlm.value.model = val
    }
  },
})

const handleSave = async () => {
  try {
    await save()
    message.success('Chore LLM saved')
  } catch (e) {
    message.error(`Failed to save: ${e}`)
  }
}

const handleClear = async () => {
  try {
    await clear()
    message.success('Chore LLM cleared')
  } catch (e) {
    message.error(`Failed to clear: ${e}`)
  }
}
</script>

<template>
  <div class="settings-view">
    <n-card title="Chore LLM" size="small">
      <template #header-extra>
        <span class="hint">Used for background tasks (e.g. MCP tool display names)</span>
      </template>
      <n-form>
        <n-form-item label="Provider">
          <n-select
            v-model:value="selectedProvider"
            :options="providerOptions"
            placeholder="Select a provider"
            clearable
          />
        </n-form-item>
        <n-form-item label="Model">
          <n-select
            v-model:value="selectedModel"
            :options="modelOptions"
            placeholder="Select a model"
            :disabled="!selectedProvider"
          />
        </n-form-item>
        <n-space justify="end">
          <n-button @click="handleClear">Clear</n-button>
          <n-button type="primary" @click="handleSave">Save</n-button>
        </n-space>
      </n-form>
    </n-card>
  </div>
</template>

<style scoped>
.settings-view {
  padding: 16px;
  height: 100%;
  overflow: auto;
  box-sizing: border-box;
}

.hint {
  font-size: 0.85em;
  opacity: 0.6;
}
</style>
```

- [ ] **Step 2: Register the route**

In `src/router/index.ts`, add the import and route:

```ts
import SettingsView from '../views/SettingsView.vue'
```

Add to the `routes` array (after the `/mcp` route):

```ts
		{
			path: '/settings',
			name: 'settings',
			component: SettingsView
		}
```

- [ ] **Step 3: Add the sidebar entry**

In `src/App.vue`:
- Add to the `@vicons/fluent` import:
```ts
  Settings24Regular,
```
- Add after the `/mcp` `router-link` block (before the closing `</div>` of `.sidebar`):
```html
              <router-link to="/settings" active-class="sidebar-item-active">
                <div class="sidebar-item">
                  <n-icon size="24"><Settings24Regular /></n-icon>
                </div>
              </router-link>
```

- [ ] **Step 4: Verify types**

Run: `npx vue-tsc --noEmit`
Expected: no NEW errors in the three touched files.

- [ ] **Step 5: Manual smoke test**

Run: `npm run tauri dev`
- Navigate to `/settings`; provider + model dropdowns populate from registered providers.
- Pick a provider + model, Save, restart app — selection persists (read from `configs.toml`).
- Clear works.

- [ ] **Step 6: Commit**

```bash
git add src/views/SettingsView.vue src/router/index.ts src/App.vue
git commit -m "feat(frontend): add Settings view with Chore LLM selector"
```

---

### Task 7: Frontend — display-name cache + enrichment (TDD for cache logic)

**Files:**
- Create: `src/libs/toolDisplayNames.ts`
- Create: `src/__tests__/libs/tool-display-names.test.ts`
- Modify: `src/stores/mcp.ts`

**Interfaces:**
- Consumes: `mcpGenerateToolDisplayNames` (Task 4), `RegisteredTool`, `ServerConfig` (Task 4).
- Produces: `loadDisplayNameCache()`, `getCachedDisplayName(desc)`, `cacheDisplayNames(entries)`, `hashDescription(desc)`, `enrichDisplayNames(tools, servers)`.

- [ ] **Step 1: Write failing tests for the cache/hash helpers**

Create `src/__tests__/libs/tool-display-names.test.ts`:

```ts
import { describe, it, expect, beforeEach } from 'vitest'
import {
  hashDescription,
  loadDisplayNameCache,
  getCachedDisplayName,
  cacheDisplayNames,
} from '../../libs/toolDisplayNames'

describe('hashDescription', () => {
  it('is deterministic for the same input', () => {
    expect(hashDescription('hello')).toBe(hashDescription('hello'))
  })

  it('differs for different input', () => {
    expect(hashDescription('hello')).not.toBe(hashDescription('world'))
  })
})

describe('display name cache', () => {
  beforeEach(() => {
    localStorage.clear()
  })

  it('returns undefined when not cached', () => {
    expect(getCachedDisplayName('desc')).toBeUndefined()
  })

  it('round-trips a cached entry', () => {
    cacheDisplayNames({ foo: 'Bar Do Thing' })
    expect(getCachedDisplayName('foo')).toBe('Bar Do Thing')
  })

  it('persists across reloads of the cache', () => {
    cacheDisplayNames({ foo: 'Bar Do Thing' })
    const fresh = loadDisplayNameCache()
    expect(fresh.foo).toBe('Bar Do Thing')
  })
})
```

- [ ] **Step 2: Run the test to verify it fails**

Run: `npm test -- tool-display-names`
Expected: FAIL — cannot resolve `../../libs/toolDisplayNames`.

- [ ] **Step 3: Implement the cache helpers**

Create `src/libs/toolDisplayNames.ts`:

```ts
import { mcpGenerateToolDisplayNames, type ToolDisplayNameInput } from './commands'
import type { RegisteredTool, ServerConfig } from './types'

const CACHE_KEY = 'mcp:tool_display_names'
const inflight = new Map<string, Promise<void>>()

export function hashDescription(desc: string): string {
  let h = 0x811c9dc5
  for (let i = 0; i < desc.length; i++) {
    h ^= desc.charCodeAt(i)
    h = Math.imul(h, 0x01000193)
  }
  return (h >>> 0).toString(16)
}

export function loadDisplayNameCache(): Record<string, string> {
  try {
    const raw = localStorage.getItem(CACHE_KEY)
    return raw ? JSON.parse(raw) : {}
  } catch {
    return {}
  }
}

export function getCachedDisplayName(desc: string): string | undefined {
  if (!desc) return undefined
  return loadDisplayNameCache()[hashDescription(desc)]
}

export function cacheDisplayNames(entries: Record<string, string>): void {
  const cache = loadDisplayNameCache()
  for (const [k, v] of Object.entries(entries)) cache[k] = v
  localStorage.setItem(CACHE_KEY, JSON.stringify(cache))
}

export async function enrichDisplayNames(
  tools: RegisteredTool[],
  servers: ServerConfig[]
): Promise<void> {
  const cache = loadDisplayNameCache()
  const serverNameOf = (id?: string): string =>
    servers.find((s) => s.id === id)?.name ?? ''

  for (const tool of tools) {
    const key = hashDescription(tool.description ?? '')
    if (cache[key]) tool.displayName = cache[key]
  }

  const uncached = tools.filter((t) => !t.displayName)
  if (uncached.length === 0) return

  const dedupeKey = uncached.map((t) => t.name).join('|')
  if (inflight.has(dedupeKey)) {
    await inflight.get(dedupeKey)
    return
  }

  const task = (async () => {
    const inputs: ToolDisplayNameInput[] = uncached.map((t) => ({
      serverName: serverNameOf(t.serverId),
      toolName: t.name,
      description: t.description,
    }))
    try {
      const result = await mcpGenerateToolDisplayNames(inputs)
      const toCache: Record<string, string> = {}
      for (const tool of uncached) {
        const name = result[tool.name]
        if (name) {
          tool.displayName = name
          toCache[hashDescription(tool.description ?? '')] = name
        }
      }
      if (Object.keys(toCache).length > 0) cacheDisplayNames(toCache)
    } catch (e) {
      console.error('[mcp] failed to enrich display names:', e)
    }
  })()

  inflight.set(dedupeKey, task)
  try {
    await task
  } finally {
    inflight.delete(dedupeKey)
  }
}
```

- [ ] **Step 4: Run the test to verify it passes**

Run: `npm test -- tool-display-names`
Expected: PASS — 5 tests.

- [ ] **Step 5: Wire enrichDisplayNames into the store**

In `src/stores/mcp.ts`:
- Add import near the top (with the other `../libs/...` imports):
```ts
import { enrichDisplayNames } from '../libs/toolDisplayNames'
```
- In `refreshAllTools`, after `tools.value = entries.map(...)` (the line that assigns `tools.value`), add:
```ts
  await enrichDisplayNames(tools.value, servers.value)
```

The resulting tail of `refreshAllTools`:
```ts
    try {
      await registryRefresh()
      const entries = await registryListTools()
      tools.value = entries.map((tool) => ({
        name: tool.name,
        description: tool.description,
        inputSchema: tool.inputSchema as RegisteredTool['inputSchema'],
        annotations: tool.annotations,
        metadata: tool.metadata,
        serverId: typeof tool.metadata?.server_id === 'string' ? tool.metadata.server_id : undefined,
        originalName: typeof tool.metadata?.original_name === 'string' ? tool.metadata.original_name : undefined,
        enabled: tool.enabled,
      }))
      await enrichDisplayNames(tools.value, servers.value)
    } catch (e) {
      console.error('Failed to refresh registry tools:', e)
    }
```

- [ ] **Step 6: Verify types + tests**

Run: `npx vue-tsc --noEmit && npm test`
Expected: no NEW type errors; all tests pass.

- [ ] **Step 7: Commit**

```bash
git add src/libs/toolDisplayNames.ts src/__tests__/libs/tool-display-names.test.ts src/stores/mcp.ts
git commit -m "feat(mcp): cache + eager LLM display-name enrichment for tools"
```

---

### Task 8: Frontend — show displayName in McpServerDetails

**Files:**
- Modify: `src/components/McpServerDetails.vue`

**Interfaces:**
- Consumes: `RegisteredTool.displayName` (set by Task 7).

- [ ] **Step 1: Update the tools table Name column**

In `src/components/McpServerDetails.vue`, replace the `toolColumns` "Name" column to render `displayName` with a tooltip of the raw `name`:

```ts
const toolColumns = [
  {
    title: 'Name',
    key: 'name',
    render(row: RegisteredTool) {
      return h(
        'span',
        { title: row.name },
        row.displayName || row.name
      )
    },
  },
  {
    title: 'Description',
    key: 'description',
  },
  {
    title: 'Actions',
    key: 'actions',
    width: 120,
    render(row: RegisteredTool) {
      return h(
        NButton,
        {
          size: 'small',
          quaternary: true,
          circle: true,
          onClick: () => handleTestTool(row),
        },
        {
          icon: () =>
            h(NIcon, null, { default: () => h(PlugConnected24Regular) }),
        }
      )
    },
  },
]
```

- [ ] **Step 2: Verify types**

Run: `npx vue-tsc --noEmit`
Expected: no NEW errors in `McpServerDetails.vue`.

- [ ] **Step 3: Manual end-to-end test**

Run: `npm run tauri dev`
- In `/settings`, configure the Chore LLM (provider + model with a valid key), Save.
- Navigate to `/mcp`; select a connected server. Tool rows show friendly names (e.g. `Filesystem Read File`) after the first load; raw name appears on hover.
- Restart the app — names appear instantly from cache (no second LLM call for cached tools; verify via network/devtools).

- [ ] **Step 4: Commit**

```bash
git add src/components/McpServerDetails.vue
git commit -m "feat(mcp): show LLM display name in tools table"
```
