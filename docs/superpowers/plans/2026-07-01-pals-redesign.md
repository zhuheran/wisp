# Pals Redesign Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Upgrade Pals from a single-character selector to a multi-role collaborative dialogue system with `@` mention routing and a director orchestration layer.

**Architecture:** Multi-pal orchestration lives in Rust (Tauri commands + conversation engine). `@` mention parsing and autocomplete lives in Vue/TS. Director mechanism is a system-level orchestration layer (not a Pal). Communication: Vue sends `target_pal_ids[]` with each message, Rust routes to each pal's model/provider/prompt in order.

**Tech Stack:** Rust (Tauri 2.0, serde, rusqlite), TypeScript (Vue 3, Pinia, Vitest), Naive UI

## Global Constraints

- All new Rust code must have `#[cfg(test)] mod tests` with comprehensive unit tests
- All new TypeScript logic must have corresponding Vitest tests in `src/__tests__/`
- Frontend `@` autocomplete must debounce by 150ms
- `Message.source` type: `'user_prompted' | 'directed'`
- Single-user-message director scheduling limit: max 1 pal
- Director context window: max 10 most recent messages
- Director can only schedule pals that have been explicitly `@` mentioned by the user in any prior message
- Multi-`@` replies execute in input order; each subsequent pal sees prior pal's reply
- The `role_bio` field on Character is optional string, max 500 chars
- Conversation.default_pal_id is optional string, nullable
- All Rust errors must propagate via `Result<T, String>` for Tauri commands
- Director prompt is a system prompt template assembled in Rust, not user-configurable in V1

---

## File Structure

### Modified Files

| File | Change |
|------|--------|
| `src-tauri/src/conversation/types.rs` | Add `source` field to message types, add `target_pal_ids` to request types |
| `src-tauri/src/configs/character.rs` | Add `role_bio` field to `Character` |
| `src-tauri/src/conversation/commands.rs` | Extend `conversation_send_message` to handle `target_pal_ids`, add multi-pal orchestration loop |
| `src-tauri/src/conversation/engine.rs` | Add director orchestration step after each round |
| `src-tauri/src/commands.rs` | Register new Tauri commands |
| `src-tauri/src/lib.rs` | Register new commands |
| `src/libs/types.ts` | Extend `Message`, `Character`, `Conversation`, `ConversationSendRequest` |
| `src/components/Chat.vue` | Integrate pal autocomplete, pass `target_pal_ids`, show pal info on bubbles |
| `src/components/MessageBubble.vue` | Display pal name/avatar, source badge |
| `src/components/CharacterForm.vue` | Add `role_bio` field |
| `src/views/PalsView.vue` | Add default responder setting |
| `src/stores/chat.ts` | Track currently `@`-mentioned pals per conversation, send `target_pal_ids` |

### New Files

| File | Purpose |
|------|---------|
| `src-tauri/src/conversation/orchestrator.rs` | Multi-pal routing, director mechanism, reply sequencing |
| `src-tauri/src/conversation/director.rs` | Director prompt assembly, output parsing |
| `src/components/PalAutocomplete.vue` | `@` mention autocomplete dropdown |
| `src/components/ChatPalBar.vue` | Conversation pal member bar |
| `src/__tests__/stores/character-store.test.ts` | Character store tests |
| `src/__tests__/components/pal-autocomplete.test.ts` | PalAutocomplete component tests |

---

## Task Breakdown

### Task 1: Extend Rust data types

**Files:**
- Modify: `src-tauri/src/configs/character.rs`
- Modify: `src-tauri/src/conversation/types.rs`

**Interfaces:**
- Consumes: `Character` struct, `ConversationSendRequest` struct definitions
- Produces: Extended `Character` with `role_bio`, extended message types with `pal_id`/`pal_name`/`source`

- [ ] **Step 1: Write failing Rust tests for new Character field**

Add to `src-tauri/src/configs/character.rs`:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn character_new_sets_role_bio_default() {
        let c = Character::new("test-id".into(), "Test".into(), "desc".into(), "prompt".into(), "model".into());
        assert_eq!(c.role_bio, "");
    }

    #[test]
    fn character_serialization_includes_role_bio() {
        let c = Character {
            id: "id".into(),
            name: "n".into(),
            role_bio: "An expert code reviewer".into(),
            ..default_character()
        };
        let json = serde_json::to_value(&c).unwrap();
        assert_eq!(json["role_bio"], "An expert code reviewer");
    }
}
```

Also add a `default_character()` helper function in the test module.

- [ ] **Step 2: Run tests, verify they fail**

Run: `cd src-tauri && cargo test -- lib configs::character::tests 2>&1`
Expected: FAIL - `role_bio` field not found

- [ ] **Step 3: Add `role_bio` field to Character**

In `Character` struct, add:
```rust
#[serde(default)]
pub role_bio: String,
```

In `Character::new()`, set `role_bio: String::new()`.

- [ ] **Step 4: Write failing tests for Message source field**

In `src-tauri/src/conversation/types.rs`, add tests:
```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn message_source_defaults_to_user_prompted() {
        let m = Message {
            id: "id".into(),
            text: "hello".into(),
            source: MessageSource::UserPrompted,
            ..
        };
        assert_eq!(m.source, MessageSource::UserPrompted);
    }
}
```

- [ ] **Step 5: Add `MessageSource` enum and extend Message struct**

Add to conversation types:
```rust
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum MessageSource {
    UserPrompted,
    Directed,
}

// Add to Message:
pub source: MessageSource,
pub pal_id: Option<String>,
pub pal_name: Option<String>,
```

- [ ] **Step 6: Add `target_pal_ids` to ConversationSendRequest**

Add field to `ConversationSendRequest` struct:
```rust
pub target_pal_ids: Option<Vec<String>>,
```

- [ ] **Step 7: Add `default_pal_id` to Conversation type**

Check if Conversation type is defined in `db/types.rs`. Add field:
```rust
pub default_pal_id: Option<String>,
```

- [ ] **Step 8: Run all tests, verify pass**

Run: `cd src-tauri && cargo test 2>&1`
Expected: All tests pass

- [ ] **Step 9: Commit**

```bash
git add -A && git commit -m "feat(pals): extend data types with role_bio, message source, target_pal_ids"
```

---

### Task 2: Extend frontend TypeScript types

**Files:**
- Modify: `src/libs/types.ts`

**Interfaces:**
- Consumes: Rust-side types (mirror in TS)
- Produces: Updated TS types consumed by all UI tasks

- [ ] **Step 1: Write failing tests for new TS types**

Create `src/__tests__/stores/types.test.ts`:

```typescript
import { describe, it, expect } from 'vitest';

describe('Message source type', () => {
  it('accepts user_prompted and directed values', () => {
    const m1: Message = {
      // ... minimal required fields
      source: 'user_prompted',
    };
    const m2: Message = {
      // ... minimal required fields
      source: 'directed',
    };
    expect(m1.source).toBe('user_prompted');
    expect(m2.source).toBe('directed');
  });
});
```

- [ ] **Step 2: Update types.ts**

Add to `Message`:
```typescript
pal_id?: string;
pal_name?: string;
source: 'user_prompted' | 'directed';
```

Add to `Character`:
```typescript
role_bio: string;
```

Add to `Conversation`:
```typescript
default_pal_id?: string;
```

Update `ConversationSendRequest`:
```typescript
target_pal_ids?: string[];
```

- [ ] **Step 3: Verify existing tests still pass**

Run: `npm run test`
Expected: All existing tests pass

- [ ] **Step 4: Commit**

```bash
git add -A && git commit -m "feat(pals): extend frontend types with pal routing fields"
```

---

### Task 3: Rust multi-pal orchestrator module

**Files:**
- Create: `src-tauri/src/conversation/orchestrator.rs`
- Create: `src-tauri/src/conversation/director.rs`
- Modify: `src-tauri/src/conversation/mod.rs`

**Interfaces:**
- Consumes: `ConversationSendRequest.target_pal_ids`, `Message` with source, Character with role_bio
- Produces: Orchestrated reply loop with multi-pal routing + director check

- [ ] **Step 1: Create `director.rs` with director prompt assembly**

```rust
use crate::configs::character::Character;

pub struct DirectorDecision {
    pub should_invoke: bool,
    pub target_pal_id: Option<String>,
}

pub fn assemble_director_prompt(
    recent_messages: &[String],
    available_pals: &[Character],
) -> String {
    let pals_desc: Vec<String> = available_pals
        .iter()
        .map(|p| format!("- {}: {}", p.name, p.role_bio))
        .collect();

    format!(
        r#"You are a dialogue director. Your job is to determine if another character should join the conversation.

Available characters (only those already @mentioned by the user):
{}

Recent conversation:
{}

If another character's expertise or perspective would add value, respond with a JSON object:
{{"action": "invoke", "pal_id": "<character_id>"}}

Otherwise respond with:
{{"action": "none"}}

Director response: "#,
        pals_desc.join("\n"),
        recent_messages.join("\n")
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn assemble_director_prompt_includes_pal_names_and_bios() {
        let pals = vec![
            Character { id: "c1".into(), name: "Code Reviewer".into(), role_bio: "Expert in Rust and system design".into(), .. },
            Character { id: "c2".into(), name: "PM".into(), role_bio: "Product strategy".into(), .. },
        ];
        // Need a minimal Character constructor for tests
    }

    #[test]
    fn assemble_director_prompt_includes_recent_messages() {
        // Verify message history included
    }

    #[test]
    fn empty_pals_list_produces_prompt_with_no_options() {
        // Verify behavior with 0 available pals
    }
}
```

Since `Character` has many fields, create a test helper `fn test_character(id: &str, name: &str, role_bio: &str) -> Character` that fills defaults for the other fields.

- [ ] **Step 2: Write director.rs tests, run, verify fail, then implement**

Run: `cd src-tauri && cargo test -- lib conversation::director 2>&1`
Expected: Compile errors or test failures → implement → pass

- [ ] **Step 3: Create `orchestrator.rs` with multi-pal routing logic**

Core function:

```rust
pub struct PalReply {
    pub message_id: String,
    pub pal_id: String,
    pub pal_name: String,
    pub text: String,
    pub source: MessageSource,
}

/// Execute a multi-pal round:
/// 1. Sort target_pal_ids by input order
/// 2. For each pal, build context (previous messages + previous pal replies), call LLM
/// 3. After all pal replies, run director check
/// 4. Return all message IDs created
pub async fn orchestrate_multi_pal_round(
    app_handle: &AppHandle,
    conversation_id: &str,
    user_message_id: &str,
    target_pal_ids: Vec<String>,
    all_characters: &[Character],
    provider: &Provider,
    parameters: Option<&HashMap<String, Value>>,
) -> Result<Vec<PalReply>, String> {
    let mut replies = Vec::new();
    let mut unlocked_pal_ids: HashSet<String> = HashSet::new();

    for pal_id in &target_pal_ids {
        unlocked_pal_ids.insert(pal_id.clone());

        let pal = all_characters.iter()
            .find(|c| c.id == *pal_id)
            .ok_or_else(|| format!("Pal not found: {}", pal_id))?;

        // Build context: existing conversation + previous pal replies in this round
        let context = build_context_for_pal(conversation_id, &replies, pal)?;

        // Call LLM with pal's model, prompt, params
        let reply_text = call_llm_with_pal_config(
            app_handle, &context, pal, provider, parameters,
        ).await?;

        // Store reply
        let reply = PalReply {
            message_id: format!("pal-{}-{}", pal_id, user_message_id),
            pal_id: pal_id.clone(),
            pal_name: pal.name.clone(),
            text: reply_text,
            source: MessageSource::UserPrompted,
        };
        replies.push(reply);
    }

    // Director check (after all pal replies)
    let director_reply = run_director_check(
        app_handle, conversation_id, user_message_id,
        &replies, all_characters, &unlocked_pal_ids,
        provider, parameters,
    ).await?;
    if let Some(reply) = director_reply {
        replies.push(reply);
    }

    Ok(replies)
}
```

- [ ] **Step 4: Write orchestrator tests**

Test cases:
- Empty target_pal_ids → no pal replies, only runs director (but no unlocked pals → no action)
- Single pal → one reply, then director check
- Multiple pals in order → sequential replies
- Director invokes a previously @mentioned pal
- Pal ID not found → error
- Director decides not to invoke → no additional reply

- [ ] **Step 5: Implement `build_context_for_pal` helper**

Reads conversation history from DB, appends previous pal replies from current round. Returns a Vec<Message> with appropriate roles.

```rust
fn build_context_for_pal(
    conversation_id: &str,
    previous_replies: &[PalReply],
    pal: &Character,
) -> Result<Vec<Message>, String> {
    // 1. Load existing conversation history (last N messages)
    // 2. Append previous pal replies in this round as assistant messages
    // 3. Prepend pal's system_prompt as a system message
    // Return combined context
}
```

- [ ] **Step 6: Implement `run_director_check`**

After all pal replies, assemble director prompt with recent conversation + unlocked pals, call LLM, parse JSON response. If `action: invoke`, call that pal's model and return a PalReply with `source: Directed`.

```rust
async fn run_director_check(
    app_handle: &AppHandle,
    conversation_id: &str,
    user_message_id: &str,
    pal_replies: &[PalReply],
    all_characters: &[Character],
    unlocked_pal_ids: &HashSet<String>,
    provider: &Provider,
    parameters: Option<&HashMap<String, Value>>,
) -> Result<Option<PalReply>, String> {
    // 1. Filter unlocked_pal_ids to actual Character objects
    // 2. Get recent messages (last 5-10)
    // 3. Assemble director prompt
    // 4. Call LLM (using same provider/model as default responder or configurable)
    // 5. Parse JSON response
    // 6. If invoke, create PalReply with source: Directed
    // 7. Return None if action: none
}
```

- [ ] **Step 7: Add `orchestrator` and `director` modules to `conversation/mod.rs`**

```rust
pub mod orchestrator;
pub mod director;
```

- [ ] **Step 8: Run all Rust tests, verify pass**

Run: `cd src-tauri && cargo test 2>&1`
Expected: All tests pass

- [ ] **Step 9: Commit**

```bash
git add -A && git commit -m "feat(pals): add multi-pal orchestrator and director modules"
```

---

### Task 4: Integrate orchestrator into conversation commands

**Files:**
- Modify: `src-tauri/src/conversation/commands.rs`
- Modify: `src-tauri/src/conversation/engine.rs`

**Interfaces:**
- Consumes: `orchestrator::orchestrate_multi_pal_round`, director module
- Produces: Modified `conversation_send_message` that routes to orchestrator when `target_pal_ids` is present

- [ ] **Step 1: Write integration test for send_message with target_pal_ids**

Add test module in `commands.rs`:
```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn send_message_with_empty_target_pal_ids_defaults_to_single_pal() {
        // Send message with target_pal_ids = None
        // Verify it uses default_responder
    }

    #[test]
    fn send_message_with_target_pal_ids_triggers_orchestrator() {
        // Send message with target_pal_ids = ["pal1", "pal2"]
        // Verify orchestrator is called with both pal IDs
    }
}
```

- [ ] **Step 2: Modify `conversation_send_message` to branch on target_pal_ids**

```rust
pub async fn conversation_send_message(
    app_handle: AppHandle,
    request: ConversationSendRequest,
) -> Result<String, String> {
    let user_message_id = Uuid::new_v4().to_string();

    // Create and insert user message (existing logic)
    let user_message = Message {
        id: user_message_id.clone(),
        text: request.text.clone(),
        reasoning: None,
        sender: MessageRole::User,
        // ... existing fields, plus:
        source: MessageSource::UserPrompted,
        pal_id: None,
        pal_name: None,
    };

    // Insert user message (existing logic)
    // ...

    if let Some(target_pal_ids) = request.target_pal_ids {
        if !target_pal_ids.is_empty() {
            // Multi-pal orchestration path
            let state = /* get state */;
            let characters = state.config_manager.get_characters();
            let replies = orchestrator::orchestrate_multi_pal_round(
                &app_handle,
                &request.conversation_id,
                &user_message_id,
                target_pal_ids,
                &characters,
                &request.provider,
                request.parameters.as_ref(),
            ).await?;

            // Emit each reply as event
            for reply in &replies {
                emit_message_event(&app_handle, &request.conversation_id, reply)?;
            }

            return Ok(replies.last().map(|r| r.message_id.clone()).unwrap_or(user_message_id));
        }
    }

    // Fallback: single default responder path (existing run_conversation_rounds)
    run_conversation_rounds(
        app_handle,
        request.conversation_id,
        user_message_id,
        request.model,
        request.provider,
        request.parameters,
        request.character,
    ).await
}
```

- [ ] **Step 3: Implement `emit_message_event` helper**

Emits `message_created` and `message_updated` events for pal replies, similar to existing event emission in the single-pal path.

- [ ] **Step 4: Update `run_conversation_rounds` to set source on assistant messages**

In the single-pal path, set `source: MessageSource::UserPrompted` on assistant messages and `pal_id`/`pal_name` from the character.

- [ ] **Step 5: Write tests for command routing logic**

```rust
#[test]
fn empty_target_pal_ids_falls_back_to_single_pal_path() {
    // Verify
}

#[test]
fn non_empty_target_pal_ids_routes_to_orchestrator() {
    // Verify
}
```

- [ ] **Step 6: Run all Rust tests, verify pass**

Run: `cd src-tauri && cargo test 2>&1`
Expected: All pass

- [ ] **Step 7: Commit**

```bash
git add -A && git commit -m "feat(pals): integrate orchestrator into conversation_send_message"
```

---

### Task 5: Frontend @ autocomplete component

**Files:**
- Create: `src/components/PalAutocomplete.vue`
- Modify: `src/components/Chat.vue`
- Create: `src/__tests__/components/pal-autocomplete.test.ts`

**Interfaces:**
- Consumes: `characterStore.characters`
- Produces: `PalAutocomplete` component with `@` trigger, keyboard navigation, mention insertion

- [ ] **Step 1: Write component test**

```typescript
import { mount } from '@vue/test-utils';
import { describe, it, expect, vi } from 'vitest';

describe('PalAutocomplete', () => {
  it('shows dropdown when @ is typed', async () => {
    const wrapper = mount(PalAutocomplete, {
      props: { modelValue: '@' },
    });
    expect(wrapper.find('.pal-autocomplete-dropdown').exists()).toBe(true);
  });

  it('filters pals by typed text after @', async () => {
    // Type "@cod" and verify only "Coder" appears (not "Designer")
  });

  it('inserts mention on Enter', async () => {
    // Press Enter on selected item, verify emit
  });

  it('dismisses on Escape', async () => {
    // Verify dropdown closes
  });
});
```

- [ ] **Step 2: Implement PalAutocomplete.vue**

```vue
<script setup lang="ts">
import { ref, computed, watch } from 'vue'
import { useCharacterStore } from '../stores/character'

const props = defineProps<{
  modelValue: string
}>()

const emit = defineEmits<{
  'update:modelValue': [value: string]
  'mention': [palId: string, palName: string]
}>()

const characterStore = useCharacterStore()
const showDropdown = ref(false)
const searchText = ref('')
const selectedIndex = ref(0)

const filteredPals = computed(() => {
  if (!searchText.value) return characterStore.characters
  const q = searchText.value.toLowerCase()
  return characterStore.characters.filter(
    c => c.name.toLowerCase().includes(q) || (c.alias?.toLowerCase().includes(q))
  )
})

let debounceTimer: ReturnType<typeof setTimeout>

function handleInput(value: string) {
  const atIndex = value.lastIndexOf('@')
  if (atIndex >= 0) {
    const afterAt = value.slice(atIndex + 1)
    searchText.value = afterAt
    showDropdown = true
    selectedIndex.value = 0
  } else {
    showDropdown = false
  }
  emit('update:modelValue', value)
}

function selectPal(palId: string, palName: string) {
  const value = props.modelValue
  const atIndex = value.lastIndexOf('@')
  const newValue = value.slice(0, atIndex) + `@${palName} `
  emit('update:modelValue', newValue)
  emit('mention', palId, palName)
  showDropdown = false
}

function onKeyDown(e: KeyboardEvent) {
  if (!showDropdown) return
  if (e.key === 'ArrowDown') {
    e.preventDefault()
    selectedIndex.value = Math.min(selectedIndex.value + 1, filteredPals.value.length - 1)
  } else if (e.key === 'ArrowUp') {
    e.preventDefault()
    selectedIndex.value = Math.max(selectedIndex.value - 1, 0)
  } else if (e.key === 'Enter' && filteredPals.value.length > 0) {
    e.preventDefault()
    selectPal(filteredPals.value[selectedIndex.value].id, filteredPals.value[selectedIndex.value].name)
  } else if (e.key === 'Escape') {
    showDropdown = false
  }
}
</script>
```

- [ ] **Step 3: Integrate into Chat.vue input area**

Wrap the input area with PalAutocomplete, listen to `@mention` events to track which pals have been mentioned.

```typescript
// In Chat.vue
const mentionedPalIds = ref<Set<string>>(new Set())

function onMention(palId: string, palName: string) {
  mentionedPalIds.value.add(palId)
}

function parseAtMentions(text: string): string[] {
  const regex = /@(\w+)/g
  const ids: string[] = []
  let match
  while ((match = regex.exec(text)) !== null) {
    const pal = characterStore.characters.find(
      c => c.name === match[1] || c.alias === match[1]
    )
    if (pal) ids.push(pal.id)
  }
  return ids
}
```

- [ ] **Step 4: Run tests, verify pass**

Run: `npm run test 2>&1`
Expected: All tests pass

- [ ] **Step 5: Commit**

```bash
git add -A && git commit -m "feat(pals): add @ autocomplete component and Chat.vue integration"
```

---

### Task 6: Message bubble pal info display

**Files:**
- Modify: `src/components/MessageBubble.vue`

**Interfaces:**
- Consumes: `Message.pal_id`, `Message.pal_name`, `Message.source`
- Produces: Visual display of pal identity on each message bubble

- [ ] **Step 1: Write component test**

```typescript
describe('MessageBubble pal display', () => {
  it('shows pal name and icon when pal_id is set', () => {
    // Mount with message that has pal_id and pal_name
    // Verify pal name is visible
  });

  it('shows "directed" badge when source is directed', () => {
    // Mount with source: 'directed'
    // Verify badge appears
  });

  it('hides pal section when pal_id is absent', () => {
    // Mount without pal_id
    // Verify no pal info shown
  });
});
```

- [ ] **Step 2: Implement pal display in MessageBubble.vue**

Add to template:
```vue
<div v-if="message.pal_id" class="message-pal-header">
  <n-icon size="16"><Bot20Regular /></n-icon>
  <n-text depth="2" style="font-size: 12px; font-weight: 600;">
    {{ message.pal_name }}
  </n-text>
  <n-tag v-if="message.source === 'directed'" size="tiny" :bordered="false">
    🎬 directed
  </n-tag>
  <n-tag v-else-if="message.source === 'user_prompted'" size="tiny" :bordered="false">
    📍 mentioned
  </n-tag>
</div>
```

- [ ] **Step 3: Run tests, verify pass**

Run: `npm run test 2>&1`
Expected: All pass

- [ ] **Step 4: Commit**

```bash
git add -A && git commit -m "feat(pals): show pal identity and source badge on message bubbles"
```

---

### Task 7: Character store and form updates

**Files:**
- Modify: `src/stores/character.ts`
- Modify: `src/components/CharacterForm.vue`
- Modify: `src/views/PalsView.vue`
- Create: `src/__tests__/stores/character-store.test.ts`

**Interfaces:**
- Consumes: Extended `Character` type with `role_bio`
- Produces: Save/load `role_bio`, default responder selection UI

- [ ] **Step 1: Write store tests**

```typescript
import { setActivePinia, createPinia } from 'pinia';
import { useCharacterStore } from '../../stores/character';

describe('characterStore', () => {
  beforeEach(() => {
    setActivePinia(createPinia());
  });

  it('createCharacter sets role_bio from input', () => {
    const store = useCharacterStore();
    // Mock the Tauri command
    // Call createCharacter with role_bio
    // Verify stored character has role_bio
  });

  it('updateCharacter preserves role_bio when not changed', () => {
    // Create character with role_bio, update other fields, verify role_bio unchanged
  });
});
```

- [ ] **Step 2: Update CharacterForm.vue**

Add a new form section after System Prompt:
```vue
<n-card title="Role Bio" size="small" style="margin-top: 16px">
  <n-text depth="3" style="margin-bottom: 8px; display: block">
    A brief description used by the director to decide when to invite this pal.
    Keep it under 500 characters.
  </n-text>
  <n-input
    v-model:value="form.role_bio"
    type="textarea"
    placeholder="e.g., Expert in Rust backend and system architecture. Good at code review."
    :autosize="{ minRows: 2, maxRows: 4 }"
    :maxlength="500"
    show-count
  />
</n-card>
```

- [ ] **Step 3: Update PalsView.vue with default responder toggle**

In the list panel, add a "Set as Default" button or star icon:
```vue
<n-button
  v-if="char.id !== currentDefaultId"
  text
  size="tiny"
  @click="setDefaultResponder(char.id)"
>
  <template #icon><n-icon><Star24Regular /></n-icon></template>
</n-button>
<n-button v-else text disabled size="tiny">
  <template #icon><n-icon color="#ffd700"><Star24Filled /></n-icon></template>
</n-button>
```

Add action to store:
```typescript
const setDefaultResponder = async (characterId: string) => {
  // Save to global config via Tauri command
}
```

- [ ] **Step 4: Create new Tauri command `configs_set_default_responder`**

In Rust commands.rs:
```rust
#[tauri::command]
pub async fn configs_set_default_responder(
    app_handle: AppHandle,
    character_id: Option<String>,
) -> Result<(), String> {
    let state = app_handle.state::<Mutex<AppData>>();
    let mut state = state.lock().unwrap();
    state.config_manager.set_default_responder(character_id)
        .map_err(|e| e.to_string())
}
```

Add to `ConfigManager`:
```rust
pub fn set_default_responder(&self, character_id: Option<String>) -> Result<(), ConfigError> {
    let mut configs = self.configs.lock().unwrap();
    configs.default_responder_id = character_id;
    std::mem::drop(configs);
    self.save()
}
```

- [ ] **Step 5: Run all tests**

Run: `npm run test && cd src-tauri && cargo test 2>&1`
Expected: All tests pass

- [ ] **Step 6: Commit**

```bash
git add -A && git commit -m "feat(pals): add role_bio field and default responder setting"
```

---

### Task 8: ChatPalBar component

**Files:**
- Create: `src/components/ChatPalBar.vue`
- Modify: `src/components/Chat.vue`

**Interfaces:**
- Consumes: List of pal IDs/names that have been @mentioned in current conversation
- Produces: Visual bar showing active pal members

- [ ] **Step 1: Write component test**

```typescript
describe('ChatPalBar', () => {
  it('shows avatar icons for each member', () => {
    const pals = [{ id: '1', name: 'Coder' }, { id: '2', name: 'PM' }];
    const wrapper = mount(ChatPalBar, { props: { pals } });
    expect(wrapper.findAll('.pal-avatar').length).toBe(2);
  });

  it('shows pal name on hover', async () => {
    // Hover over first avatar, verify tooltip shows "Coder"
  });

  it('shows empty state when no pals', () => {
    const wrapper = mount(ChatPalBar, { props: { pals: [] } });
    expect(wrapper.find('.pal-bar-empty').exists()).toBe(true);
  });
});
```

- [ ] **Step 2: Implement ChatPalBar.vue**

```vue
<script setup lang="ts">
import { useCharacterStore } from '../stores/character'

const props = defineProps<{
  palIds: string[]
}>()

const characterStore = useCharacterStore()
const activePals = computed(() =>
  props.palIds
    .map(id => characterStore.characters.find(c => c.id === id))
    .filter(Boolean)
)
</script>

<template>
  <div class="pal-bar">
    <template v-if="activePals.length > 0">
      <div v-for="pal in activePals" :key="pal.id" class="pal-avatar"
           :title="`${pal.name} (${pal.model_id})`">
        <n-icon size="20"><Bot20Regular /></n-icon>
        <n-text class="pal-name-label">{{ pal.name }}</n-text>
      </div>
    </template>
    <n-text v-else depth="3" class="pal-bar-empty" style="font-size: 12px">
      No pals mentioned yet. Type @ to invite one.
    </n-text>
  </div>
</template>

<style scoped>
.pal-bar {
  display: flex;
  gap: 4px;
  padding: 4px 8px;
  align-items: center;
  border-bottom: 1px solid v-bind('theme.borderColor');
}
.pal-avatar {
  display: flex;
  align-items: center;
  gap: 4px;
  padding: 2px 8px;
  border-radius: 12px;
  background: v-bind('theme.hoverColor');
  cursor: default;
}
</style>
```

- [ ] **Step 3: Integrate into Chat.vue**

Add to top of chat area:
```vue
<ChatPalBar :pal-ids="mentionedPalIds" />
```

Track `mentionedPalIds` across messages in the conversation (might need to persist to conversation metadata).

- [ ] **Step 4: Run tests, verify pass**

Run: `npm run test 2>&1`
Expected: All pass

- [ ] **Step 5: Commit**

```bash
git add -A && git commit -m "feat(pals): add ChatPalBar component showing active pal members"
```

---

### Task 9: Store integration — track mentioned pals per conversation

**Files:**
- Modify: `src/stores/chat.ts`
- Modify: `src/stores/character.ts`

**Interfaces:**
- Consumes: Existing chat store structure
- Produces: `mentionedPalIds` tracking, `target_pal_ids` in `sendMessage` calls

- [ ] **Step 1: Write store tests**

```typescript
describe('chatStore pal integration', () => {
  it('tracks mentioned pal IDs across messages', () => {
    const store = useChatStore();
    store.addMentionedPal('pal-1');
    store.addMentionedPal('pal-2');
    expect(store.mentionedPalIds).toEqual(new Set(['pal-1', 'pal-2']));
  });

  it('sendMessage includes target_pal_ids from @ mentions', () => {
    // Mock conversationSendMessage
    // Send message with @ mentions
    // Verify target_pal_ids passed to command
  });
});
```

- [ ] **Step 2: Implement pal tracking in chat store**

Add to store state:
```typescript
const mentionedPalIds = ref<Set<string>>(new Set())
```

Add method:
```typescript
function addMentionedPal(palId: string) {
  mentionedPalIds.value.add(palId)
}

function getMentionedPalsForConversation(conversationId: string): string[] {
  // Return array of pal IDs mentioned in this conversation
  return Array.from(mentionedPalIds.value)
}
```

- [ ] **Step 3: Modify `sendMessage` to pass target_pal_ids**

```typescript
const sendMessage = async (message, callbacks, parentMessageId) => {
  // ... existing code ...

  const targetPalIds = parseAtMentions(message.text)

  await Commands.conversationSendMessage({
    // ... existing fields ...
    target_pal_ids: targetPalIds.length > 0 ? targetPalIds : undefined,
  })

  // Track mentioned pals
  targetPalIds.forEach(id => addMentionedPal(id))
}
```

- [ ] **Step 4: Run tests**

Run: `npm run test 2>&1`
Expected: All pass

- [ ] **Step 5: Commit**

```bash
git add -A && git commit -m "feat(pals): integrate pal mention tracking and target_pal_ids in chat store"
```

---

### Task 10: Chat.vue enhanced message sending with multi-pal support

**Files:**
- Modify: `src/components/Chat.vue`

**Interfaces:**
- Consumes: PalAutocomplete, mentionedPalIds tracking, updated chat store
- Produces: Working multi-pal @ mention flow from input to sending

- [ ] **Step 1: Write integration test**

```typescript
describe('Chat multi-pal sending', () => {
  it('extracts multiple pal IDs from @ mentions', () => {
    // Call parseAtMentions("@coder @pm check this")
    // Verify returns ['coder-id', 'pm-id']
  });

  it('skips non-existent pal names', () => {
    // Call parseAtMentions("@unknown check")
    // Verify returns []
  });

  it('handles mix of known and unknown mentions', () => {
    // "@coder @unknown @pm"
    // Returns only ['coder-id', 'pm-id']
  });
});
```

- [ ] **Step 2: Integrate all pieces in Chat.vue**

In the send flow:
```typescript
const handleSendMessage = () => {
  const text = chatStore.userInput
  const targetPalIds = parseAtMentions(text)

  // If has @mentions, send with target_pal_ids
  chatStore.sendMessage(
    {
      text: text,
      sender: MessageRole.User,
      images: images,
      timestamp: Date.now() / 1000,
    },
    {
      beforeSend: (botMessageId) => { /* existing */ },
      onReceiving: (chunk, isReasoning) => { /* existing */ },
      onFinish: (text, reasoning) => { /* existing */ },
    },
    undefined
  )

  // Track mentioned pals
  targetPalIds.forEach(id => {
    chatStore.addMentionedPal(id)
    // If first mention, trigger "joined" system message
    if (!chatStore.hasPalBeenMentionedBefore(id)) {
      chatStore.addSystemMessage(`🎭 ${getPalName(id)} 加入了对话`)
    }
  })
}
```

- [ ] **Step 3: Add "pal joined" system message**

In chat store:
```typescript
const palMentionHistory = ref<Set<string>>(new Set())

function hasPalBeenMentionedBefore(palId: string): boolean {
  return palMentionHistory.value.has(palId)
}
```

On first mention, insert a system message into the conversation:
```typescript
function addPalJoinedMessage(palName: string) {
  const systemMessage = {
    text: `🎭 ${palName} joined the conversation`,
    sender: MessageRole.System,
    timestamp: Math.round(Date.now() / 1000),
  }
  // Insert via existing conversation event system
}
```

- [ ] **Step 4: Remove old character selector and default pal logic**

The current `chosenCharacterId` dropdown in Chat.vue should be removed or simplified. The character selection becomes purely `@`-driven.

Replace the character dropdown with:
```vue
<template v-if="mentionedPalIds.size > 0">
  <n-tag v-for="palId in mentionedPalIds" :key="palId" :bordered="false" closable @close="removePal(palId)">
    {{ getPalName(palId) }}
  </n-tag>
</template>
<n-text v-else depth="3" style="font-size: 12px">
  Type @ to invite a pal
</n-text>
```

- [ ] **Step 5: Run all tests, verify pass**

Run: `npm run test 2>&1`
Expected: All pass

- [ ] **Step 6: Commit**

```bash
git add -A && git commit -m "feat(pals): integrate multi-pal sending flow into Chat.vue"
```

---

### Task 11: Conversation creation with default responder

**Files:**
- Create: `src/components/CreateConversationDialog.vue` (or modify existing conversation creation flow)
- Modify: `src/components/Chat.vue`

**Interfaces:**
- Consumes: `characterStore.characters`, `providerStore.providers`
- Produces: New conversation creation dialog with default responder picker

- [ ] **Step 1: Write component test**

```typescript
describe('CreateConversationDialog', () => {
  it('shows pal selection list', () => {
    const wrapper = mount(CreateConversationDialog, {
      props: { show: true, pals: mockPals },
    });
    expect(wrapper.findAll('.pal-select-item').length).toBe(mockPals.length);
  });

  it('requires selecting at least one pal', async () => {
    // Try submitting without selection
    // Verify error shown
  });

  it('creates conversation with selected default responder', async () => {
    // Select a pal, submit
    // Verify createConversation called with default_pal_id
  });
});
```

- [ ] **Step 2: Implement dialog**

```vue
<script setup lang="ts">
const selectedPalId = ref<string | null>(null)
const conversationName = ref('')

const filteredPals = computed(() =>
  characterStore.characters.filter(c => !c.name.toLowerCase().includes('default'))
)

async function handleCreate() {
  if (!selectedPalId.value) return
  const id = await chatStore.createConversation(conversationName.value || 'New Conversation')
  // Set default responder
  await characterStore.setConversationDefaultResponder(id, selectedPalId.value)
  // Open conversation
  emit('created', id)
}
</script>
```

- [ ] **Step 3: Register Tauri command to set conversation default_pal_id**

In `commands.rs`:
```rust
#[tauri::command]
pub async fn conversation_set_default_responder(
    app_handle: AppHandle,
    conversation_id: String,
    character_id: String,
) -> Result<(), String> {
    let state = app_handle.state::<Mutex<AppData>>();
    let mut state = state.lock().unwrap();
    state.chat.set_conversation_default_responder(&conversation_id, &character_id)
        .map_err(|e| e.to_string())
}
```

- [ ] **Step 4: Update Conversation list tests**

- [ ] **Step 5: Commit**

```bash
git add -A && git commit -m "feat(pals): add conversation creation dialog with default responder selection"
```

---

### Task 12: Director backend integration

**Files:**
- Modify: `src-tauri/src/conversation/commands.rs`
- Modify: `src-tauri/src/conversation/orchestrator.rs`

**Interfaces:**
- Consumes: `director::assemble_director_prompt`, existing LLM calling infrastructure
- Produces: Fully integrated director check after each message round

- [ ] **Step 1: Write director integration tests**

```rust
#[cfg(test)]
mod tests {
    #[test]
    fn director_check_after_single_pal_reply_invokes_director() {
        // Send message with single @pal
        // Verify director check runs after pal reply
    }

    #[test]
    fn director_check_after_multi_pal_replies_invokes_once() {
        // Send message with @pal1 @pal2
        // Verify director runs exactly once after both replies
    }

    #[test]
    fn director_does_not_invoke_if_no_unlocked_pals() {
        // Director has zero unlocked pals to choose from
        // Verify director returns action: none without making LLM call
    }

    #[test]
    fn director_skips_already_replied_pals_in_current_round() {
        // @coder replies, director should not re-invoke coder
    }
}
```

- [ ] **Step 2: Implement director check in `orchestrate_multi_pal_round`**

After the pal replies loop, call `run_director_check`:

```rust
// Director check
if !unlocked_pal_ids.is_empty() {
    let director_prompt = director::assemble_director_prompt(
        &recent_messages,
        &available_pals,
    );

    let director_response = call_llm_with_prompt(
        app_handle, &director_prompt, director_model, provider, params
    ).await?;

    if let Some(pal_id) = parse_director_response(&director_response) {
        // Skip if this pal already replied this round
        if !replied_this_round.contains(&pal_id) {
            // Create directed reply
            let pal = /* find by id */;
            let reply = call_llm_with_pal_config(...).await?;
            replies.push(PalReply { source: Directed, ... });
        }
    }
}
```

- [ ] **Step 3: Implement `parse_director_response`**

```rust
fn parse_director_response(response: &str) -> Option<String> {
    // Try to extract JSON from response
    // Look for {"action": "invoke", "pal_id": "xxx"}
    // Return pal_id if found, None otherwise
}

#[cfg(test)]
mod tests {
    #[test]
    fn parses_valid_director_invoke_response() {
        let result = parse_director_response(r#"{"action": "invoke", "pal_id": "coder-1"}"#);
        assert_eq!(result, Some("coder-1".to_string()));
    }

    #[test]
    fn returns_none_for_none_action() {
        let result = parse_director_response(r#"{"action": "none"}"#);
        assert_eq!(result, None);
    }

    #[test]
    fn returns_none_for_garbled_response() {
        let result = parse_director_response("I think the coder should respond");
        assert_eq!(result, None);
    }
}
```

- [ ] **Step 4: Handle the case where target_pal_ids is None (single default responder)**

When no `target_pal_ids`, the default responder replies, then director check runs with any previously unlocked pals.

```rust
// In conversation_send_message, after single-pal path
if let Some(director_reply) = run_director_check_for_conversation(
    &app_handle, &conversation_id, &user_message_id,
    &[], &all_characters, &unlocked_pal_ids, provider, params,
).await? {
    emit_message_event(&app_handle, &conversation_id, &director_reply)?;
}
```

- [ ] **Step 5: Run all Rust tests, verify pass**

Run: `cd src-tauri && cargo test 2>&1`
Expected: All pass

- [ ] **Step 6: Commit**

```bash
git add -A && git commit -m "feat(pals): integrate director orchestration into reply flow"
```

---

### Task 13: Tauri command registration and lib.rs wiring

**Files:**
- Modify: `src-tauri/src/lib.rs`
- Modify: `src-tauri/src/commands.rs`

- [ ] **Step 1: Register new commands in lib.rs**

Add to `tauri::Builder::default().invoke_handler(tauri::generate_handler![...])`:
```rust
commands::configs_set_default_responder,
commands::conversation_set_default_responder,
```

- [ ] **Step 2: Register in frontend commands.ts**

Add:
```typescript
export async function configsSetDefaultResponder(characterId: string | null) {
    return invoke<void>('configs_set_default_responder', { characterId })
}

export async function conversationSetDefaultResponder(conversationId: string, characterId: string) {
    return invoke<void>('conversation_set_default_responder', { conversationId, characterId })
}
```

- [ ] **Step 3: Verify Tauri build compiles**

Run: `cd src-tauri && cargo build 2>&1`
Expected: Compiles without errors

- [ ] **Step 4: Commit**

```bash
git add -A && git commit -m "feat(pals): register new Tauri commands and frontend bindings"
```

---

## Verification Plan

1. **Unit tests** — Each task has its own test suite (Vitest for TS, `cargo test` for Rust)
2. **Build verification** — `cd src-tauri && cargo build` for Rust, `npm run build` for Vue/TS
3. **Manual test scenarios:**
   - Create two pals with different models/system prompts
   - Start a new conversation with one as default responder
   - Type `@pal2 hello` → verify pal2 replies using its own model
   - Type `@pal1 @pal2 both` → verify both reply in order
   - Type without @ → verify default responder replies
   - Verify director does not trigger un-@mentioned pal
   - After @mentioning a pal, verify director can schedule it later
