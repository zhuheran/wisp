<script setup lang="ts">
import {
  NDialogProvider,
  NConfigProvider,
  NModalProvider,
  NMessageProvider,
  NIcon,
  NGlobalStyle,
  useOsTheme,
  darkTheme,
  lightTheme,
} from "naive-ui";
import katex from "katex";
import { provide, computed, onMounted } from "vue";
import {
  ChatMultiple24Regular,
  Bot24Regular,
  Cube24Regular,
} from "@vicons/fluent";
import useHighlightjs from "./composables/useHighlightjs";
const hljs = useHighlightjs();

import { useOpenAI } from "./composables/useOpenAI";

import { useMermaid } from "./composables/useMermaid";
import { useVNodeRenderer } from "./composables/useMarkdown";
import { useChatStore } from "./stores/chat";
import { useProviderStore } from './stores/provider';
import { useCharacterStore } from './stores/character';

const osThemeRef = useOsTheme();
const isDark = computed(() => osThemeRef.value === "dark");
const theme = computed(() => (isDark.value ? darkTheme : lightTheme));

onMounted(() => {
  (async () => {
    provide("OpenAI", useOpenAI());
    provide("MermaidRenderer", useMermaid());
    provide("MarkdownRenderer", useVNodeRenderer());
    provide("ChatStore", useChatStore());
    const providerStore = useProviderStore();
    provide("ProviderStore", providerStore);
    providerStore.loadProviders()
    const characterStore = useCharacterStore();
    provide("CharacterStore", characterStore);
    characterStore.loadCharacters()
  })()
})

</script>

<template>
  <n-config-provider :katex="(katex as any)" :hljs="hljs" :theme="theme">
    <n-global-style />
    <n-dialog-provider>
      <n-modal-provider>
        <n-message-provider>
          <div class="container">
            <div class="sidebar">
              <router-link to="/chat" active-class="sidebar-item-active">
                <div class="sidebar-item">
                  <n-icon size="24"><ChatMultiple24Regular /></n-icon>
                </div>
              </router-link>
              <router-link to="/pals" active-class="sidebar-item-active">
                <div class="sidebar-item">
                  <n-icon size="24"><Bot24Regular /></n-icon>
                </div>
              </router-link>
              <router-link to="/providers" active-class="sidebar-item-active">
                <div class="sidebar-item">
                  <n-icon size="24"><Cube24Regular /></n-icon>
                </div>
              </router-link>
            </div>
            <div class="main-content">
              <router-view v-slot="{ Component, route }">
                <transition name="fade">
                  <keep-alive>
                    <component :is="Component" :key="route.path"/>
                  </keep-alive>
                </transition>
              </router-view>
            </div>
          </div>
        </n-message-provider>
      </n-modal-provider>
    </n-dialog-provider>
  </n-config-provider>
</template>

<style>
:root {
  font-family: Inter, Avenir, Helvetica, Arial, sans-serif;
  font-size: 16px;
  line-height: 24px;
  font-weight: 400;

  font-synthesis: none;
  text-rendering: optimizeLegibility;
  -webkit-font-smoothing: antialiased;
  -moz-osx-font-smoothing: grayscale;
  -webkit-text-size-adjust: 100%;
}

html,
body {
  margin: 0;
  padding: 0;
  width: 100%;
  height: 100%;
  overflow: hidden;
  background-color: transparent !important;
}
</style>

<style scoped>
/* .fade-enter-active,
.fade-leave-active {
  transition: opacity 0.2s ease-in-out;
}

.fade-enter-from,
.fade-leave-to {
  opacity: 0;
} */

.container {
  position: fixed;
  top: 0;
  left: 0;
  width: 100%;
  height: 100%;

  display: grid;
  grid-template-rows: 1fr;
  grid-template-columns: 64px auto;
}

.sidebar {
  grid-area: 1 / 1 / 2 / 2;
  padding: 8px;
  width: 100%;
  box-sizing: border-box;

  display: flex;
  flex-direction: column;
  gap: 8px;
}

.sidebar-item {
  display: flex;
  align-items: center;
  justify-content: center;
  color: v-bind("theme.common.textColor3");

  height: 48px;
  width: 48px;
  border-radius: v-bind("theme.common.borderRadius");
  transition: 0.2s v-bind("theme.common.cubicBezierEaseInOut");
}

.sidebar-item:hover:not(.sidebar-item-active .sidebar-item) {
  background-color: v-bind("theme.common.hoverColor");
}

.sidebar-item-active svg {
  color: v-bind("theme.common.textColor1");
}

.main-content {
  grid-area: 1 / 2 / 3 / 2;
  position: relative;
}
</style>
