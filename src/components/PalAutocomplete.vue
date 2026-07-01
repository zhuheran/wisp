<script setup lang="ts">
import { ref, computed, watch, onMounted, onUnmounted } from "vue";
import type { Character } from "../libs/types";

const props = defineProps<{
  modelValue: string;
  characters: Character[];
}>();

const emit = defineEmits<{
  "update:modelValue": [value: string];
  mention: [palId: string, palName: string];
}>();

const showDropdown = ref(false);
const searchText = ref("");
const selectedIndex = ref(0);
const inputEl = ref<HTMLInputElement | HTMLTextAreaElement | null>(null);

const filteredPals = computed(() => {
  if (!searchText.value) return props.characters;
  const q = searchText.value.toLowerCase();
  return props.characters.filter(
    (c) =>
      c.name.toLowerCase().includes(q) ||
      (c.alias?.toLowerCase().includes(q))
  );
});

function detectAt(value: string) {
  const atIndex = value.lastIndexOf("@");
  if (atIndex >= 0) {
    // Check that there's no word character immediately before the @
    // to avoid matching mid-word @ symbols
    if (atIndex > 0 && /\w/.test(value[atIndex - 1])) {
      showDropdown.value = false;
      return;
    }
    const afterAt = value.slice(atIndex + 1);
    // If after @ there's a space or it's empty, we still show the dropdown
    // but if there are non-word characters (except empty/null), hide
    if (/[\s]/.test(afterAt[0]) && afterAt.length > 0) {
      showDropdown.value = false;
      return;
    }
    searchText.value = afterAt.replace(/\s.*$/, ""); // take only up to first space
    showDropdown.value = true;
    selectedIndex.value = 0;
  } else {
    showDropdown.value = false;
  }
}

let debounceTimer: ReturnType<typeof setTimeout>;

function handleInput(value: string) {
  emit("update:modelValue", value);
  clearTimeout(debounceTimer);
  debounceTimer = setTimeout(() => {
    detectAt(value);
  }, 150);
}

function selectPal(pal: Character) {
  const value = props.modelValue;
  const atIndex = value.lastIndexOf("@");
  // Find the start of the @mention (from the @ to the end of the search word)
  const afterAt = value.slice(atIndex + 1);
  const wordEnd = afterAt.search(/[\s]|$/);
  const mentionEnd = wordEnd >= 0 ? atIndex + 1 + wordEnd : value.length;

  const newValue =
    value.slice(0, atIndex) + `@${pal.name} ` + value.slice(mentionEnd);

  emit("update:modelValue", newValue);
  emit("mention", pal.id, pal.name);
  showDropdown.value = false;
}

function onKeyDown(e: KeyboardEvent) {
  if (!showDropdown.value) return;

  if (e.key === "ArrowDown") {
    e.preventDefault();
    selectedIndex.value = Math.min(
      selectedIndex.value + 1,
      filteredPals.value.length - 1
    );
  } else if (e.key === "ArrowUp") {
    e.preventDefault();
    selectedIndex.value = Math.max(selectedIndex.value - 1, 0);
  } else if (e.key === "Enter" && filteredPals.value.length > 0) {
    e.preventDefault();
    selectPal(filteredPals.value[selectedIndex.value]);
  } else if (e.key === "Escape") {
    e.preventDefault();
    showDropdown.value = false;
  }
}

function onInputFocus() {
  detectAt(props.modelValue);
}

watch(() => props.modelValue, (newVal) => {
  detectAt(newVal);
});

onMounted(() => {
  detectAt(props.modelValue);
});

onUnmounted(() => {
  clearTimeout(debounceTimer);
});

defineExpose({
  inputEl,
});
</script>

<template>
  <div class="pal-autocomplete-wrapper" @keydown="onKeyDown">
    <slot
      :handleInput="handleInput"
      :onInputFocus="onInputFocus"
      :onKeyDown="onKeyDown"
    />
    <div
      v-if="showDropdown && filteredPals.length > 0"
      class="pal-autocomplete-dropdown"
    >
      <div
        v-for="(pal, index) in filteredPals"
        :key="pal.id"
        class="pal-autocomplete-item"
        :class="{ 'pal-autocomplete-item--active': index === selectedIndex }"
        @mousedown.prevent="selectPal(pal)"
        @mouseenter="selectedIndex = index"
      >
        <span class="pal-autocomplete-item-name">{{ pal.name }}</span>
        <span v-if="pal.alias" class="pal-autocomplete-item-alias"
          >@{{ pal.alias }}</span
        >
      </div>
    </div>
  </div>
</template>

<style scoped>
.pal-autocomplete-wrapper {
  position: relative;
}

.pal-autocomplete-dropdown {
  position: absolute;
  bottom: 100%;
  left: 0;
  right: 0;
  z-index: 100;
  background: var(--n-color, #fff);
  border: 1px solid var(--n-border-color, #e0e0e0);
  border-radius: 8px;
  box-shadow: 0 4px 12px rgba(0, 0, 0, 0.15);
  max-height: 200px;
  overflow-y: auto;
  margin-bottom: 4px;
}

.pal-autocomplete-item {
  display: flex;
  align-items: center;
  gap: 8px;
  padding: 8px 12px;
  cursor: pointer;
  transition: background-color 0.15s;
}

.pal-autocomplete-item:hover,
.pal-autocomplete-item--active {
  background-color: var(--n-primary-color, #18a058);
  color: #fff;
}

.pal-autocomplete-item-name {
  font-weight: 600;
}

.pal-autocomplete-item-alias {
  font-size: 0.85em;
  opacity: 0.7;
}

.pal-autocomplete-item:hover .pal-autocomplete-item-alias,
.pal-autocomplete-item--active .pal-autocomplete-item-alias {
  opacity: 0.9;
}
</style>
