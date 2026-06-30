<script setup lang="ts">
import {
  NAvatar,
  NIcon,
  NButton,
  NFlex,
  NButtonGroup,
  NCollapse,
  NCollapseItem,
  NTag,
  NImage,
  useDialog,
  useThemeVars,
} from "naive-ui";
import {
  Chat24Regular,
  Person24Regular,
  Copy16Regular,
  Delete16Regular,
  Edit16Regular,
  ArrowClockwise16Regular,
  ChevronLeft16Regular,
  ChevronRight16Regular,
  Toolbox24Regular,
} from "@vicons/fluent";
import MarkdownRenderer from "./MarkdownRenderer.vue";
import { writeText } from "@tauri-apps/plugin-clipboard-manager";
import { MessageRole, type ImageContent } from "../libs/types";
import { useChatStore } from "../stores/chat";
import { ref, computed, useTemplateRef, watch } from "vue";
import { mixColours } from "../utils/colour";
import { useElementSize, useElementVisibility } from "@vueuse/core";
import { debounce } from "lodash";
import { Menu, MenuItem, PredefinedMenuItem } from "@tauri-apps/api/menu";

const chatStore = useChatStore();
const dialog = useDialog();
const theme = useThemeVars();

const borderColor = computed(() =>
  props.sender === MessageRole.User ? "transparent" : theme.value.borderColor
);

const backgroundColor = computed(() =>
  props.sender === MessageRole.User
    ? mixColours(theme.value.primaryColor, theme.value.baseColor, 0.3)
    : theme.value.cardColor
);

const border = computed(() => `1px solid ${borderColor.value}`);

const props = defineProps<{
  text: string;
  reasoning?: string;
  sender: MessageRole;
  timestamp: Date;
  id: string;
  over?: boolean;
  hasPrevious?: boolean;
  hasNext?: boolean;
  culling?: boolean;
  index?: number;
  images?: ImageContent[];
}>();

const emit = defineEmits<{
  (e: "resend", derive: boolean, text: string): void;
  (e: "edit"): void;
  (e: "regenerate"): void;
  (e: "previous"): void;
  (e: "next"): void;
  (e: "ready"): void;
}>();

const isStreaming = computed(() => chatStore.isStreaming);

const container = useTemplateRef<HTMLDivElement>("container");
const height = ref(0);
const rendered = ref(false);
const visible = useElementVisibility(container);

if (props.culling) {
  const size = useElementSize(container);
  watch(
    [size.height, rendered, visible],
    debounce((newVal) => {
      if (!rendered.value || isStreaming.value) return;
      height.value = Math.round(newVal[0]);
    }, 100)
  );
}

const copyMessage = async () => {
  await writeText(props.text);
  const bubble = document.querySelector(`.message-bubble[id="${props.id}"]`);
  if (bubble) {
    bubble.classList.add("copied");
    setTimeout(() => bubble.classList.remove("copied"), 500);
  }
};

const removeMessage = () => {
  dialog.warning({
    title: "Delete Message",
    content: "Are you sure you want to delete this message?",
    positiveText: "Delete",
    negativeText: "Cancel",
    onPositiveClick: async () => {
      await chatStore.deleteMessage(props.id);
      console.log("Message deleted:", props.id);
    },
  });
};

const showContextMenu = async (e: MouseEvent) => {
  e.stopPropagation();

  const menu = await Menu.new();

  const selectedText = window.getSelection()?.toString();
  if (selectedText) {
    await menu.append(
      await MenuItem.new({
        text: "Copy selected",
        action: async () => await writeText(selectedText),
      })
    );

    await menu.append(await PredefinedMenuItem.new({ item: "Separator" }));
  }

  await menu.append(
    await MenuItem.new({
      text: "Copy",
      action: () => copyMessage(),
    })
  );

  await menu.append(
    await MenuItem.new({
      text: "Delete",
      action: () => removeMessage(),
    })
  );

  await menu.append(
    await MenuItem.new({
      text: "Regenerate",
      action: () => emit("regenerate"),
    })
  );

  await menu.append(
    await MenuItem.new({
      text: "Edit",
      action: () => emit("edit"),
    })
  );

  await menu.popup();
};

const onReadyStatusChange = (ready: boolean) => {
  if (ready) emit("ready");
};

const thinkingPanelExpandedNames = ref<string[]>([]);
if (!(props.over ?? true)) thinkingPanelExpandedNames.value.push("thinking");

const footerVisible = ref(false);

// MCP Tool Call Result Parsing
// 匹配 [Tool: tool_name]\nResult: {...} 或 [Tool: tool_name]\nError: ... 格式
// 支持多工具调用、嵌套 JSON、保留工具调用前后的文本
type ParsedToolCall = {
  toolName: string;
  status: 'success' | 'error';
  result: string;
  start: number;
  end: number;
};

// 清理未配对的 <|tool_call|> 指令标签（仅清理成对标签外的残留）
const cleanToolCallTags = (text: string): string => {
  // 先清理成对标签及其内容
  let cleaned = text.replace(/<\|tool_call\|>[\s\S]*?<\|tool_call\|>/g, '');
  // 再清理残留的未配对开标签或闭标签
  cleaned = cleaned.replace(/<\|tool_call\|>/g, '');
  return cleaned;
};

// 从指定位置提取一个 JSON 值（支持嵌套），返回结束位置和原始字符串
const extractBalancedJson = (text: string, startIndex: number): { raw: string; end: number } | null => {
  if (text[startIndex] !== '{') return null;
  let depth = 0;
  let inString = false;
  let escape = false;
  for (let i = startIndex; i < text.length; i++) {
    const ch = text[i];
    if (escape) { escape = false; continue; }
    if (ch === '\\' && inString) { escape = true; continue; }
    if (ch === '"') { inString = !inString; continue; }
    if (inString) continue;
    if (ch === '{') depth++;
    else if (ch === '}') {
      depth--;
      if (depth === 0) {
        return { raw: text.substring(startIndex, i + 1), end: i + 1 };
      }
    }
  }
  return null;
};

const parsedToolCalls = computed<ParsedToolCall[]>(() => {
  const results: ParsedToolCall[] = [];
  const text = props.text;
  // 使用标志位匹配 [Tool: name]，避免误匹配 markdown 链接
  const headerPattern = /\[Tool:\s*([^\]]+)\]\s*\n(Result|Error):\s*/g;
  let m: RegExpExecArray | null;
  while ((m = headerPattern.exec(text)) !== null) {
    const toolName = m[1];
    const status = m[2] === 'Error' ? 'error' : 'success';
    const contentStart = headerPattern.lastIndex;
    let resultRaw: string;
    let resultEnd: number;
    if (status === 'success' && text[contentStart] === '{') {
      // 尝试提取平衡的嵌套 JSON
      const extracted = extractBalancedJson(text, contentStart);
      if (extracted) {
        resultRaw = extracted.raw;
        resultEnd = extracted.end;
      } else {
        // 兜底：取到行尾
        const lineEnd = text.indexOf('\n', contentStart);
        resultEnd = lineEnd === -1 ? text.length : lineEnd;
        resultRaw = text.substring(contentStart, resultEnd);
      }
    } else {
      // Error 文本或非对象 Result：取到下一个 [Tool: 头部或文本末尾
      const nextHeader = text.indexOf('[Tool:', contentStart);
      resultEnd = nextHeader === -1 ? text.length : nextHeader;
      resultRaw = text.substring(contentStart, resultEnd).trim();
    }
    results.push({
      toolName,
      status,
      result: resultRaw,
      start: m.index,
      end: resultEnd,
    });
    // 推进 lastIndex 避免重复匹配
    headerPattern.lastIndex = resultEnd;
  }
  return results;
});

const hasToolCall = computed(() => parsedToolCalls.value.length > 0);

const toolCallExpandedNames = ref<string[]>([]);

// 清理后的显示文本：移除工具调用块本身和 <|tool_call|> 指令标签，保留工具调用前后的所有文本
const displayText = computed(() => {
  if (!hasToolCall.value) {
    return cleanToolCallTags(props.text);
  }
  // 拼接工具调用块之间的文本片段
  let cleaned = '';
  let lastEnd = 0;
  for (const call of parsedToolCalls.value) {
    cleaned += props.text.substring(lastEnd, call.start);
    lastEnd = call.end;
  }
  cleaned += props.text.substring(lastEnd);
  // 移除孤立的 --- 分隔符（工具调用块前后留下的）
  cleaned = cleaned.replace(/^\s*---\s*\n?/gm, '').replace(/\n\s*---\s*$/g, '');
  return cleanToolCallTags(cleaned).trim();
});

// 工具调用块之间的文本片段（用于在折叠面板之间插入渲染）
const textSegmentsBetweenTools = computed<{ beforeFirst: string; between: string[]; afterLast: string }>(() => {
  const calls = parsedToolCalls.value;
  if (calls.length === 0) return { beforeFirst: '', between: [], afterLast: '' };
  const cleanSegment = (raw: string) => {
    let s = cleanToolCallTags(raw);
    s = s.replace(/^\s*---\s*\n?/gm, '').replace(/\n\s*---\s*$/g, '');
    return s.trim();
  };
  const beforeFirst = cleanSegment(props.text.substring(0, calls[0].start));
  const between: string[] = [];
  for (let i = 0; i < calls.length - 1; i++) {
    between.push(cleanSegment(props.text.substring(calls[i].end, calls[i + 1].start)));
  }
  const afterLast = cleanSegment(props.text.substring(calls[calls.length - 1].end));
  return { beforeFirst, between, afterLast };
});
</script>

<template>
  <div ref="container">
    <div
      v-if="!visible && height !== 0 && culling && !isStreaming"
      class="placeholder"
    ></div>
    <div v-else class="item-container">
      <n-flex align="start" :wrap="false" class="item-layout">
        <n-avatar class="avatar">
          <n-icon
            :component="sender === 'bot' ? Chat24Regular : Person24Regular"
          />
        </n-avatar>
        <div
          class="message-bubble"
          :class="sender"
          :id="id"
          :tabindex="10"
          @mouseenter="() => (footerVisible = true)"
          @mouseleave="() => (footerVisible = false)"
          @focusin="() => (footerVisible = true)"
          @focusout="() => (footerVisible = false)"
        >
          <div
            class="content-container"
            @contextmenu="
              (e) => {
                e.preventDefault();
                showContextMenu(e);
              }
            "
          >
            <div class="content">
              <!-- Images Display -->
              <div v-if="images && images.length > 0" class="images-container">
                <n-flex :wrap="true" size="small">
                  <n-image
                    v-for="(image, idx) in images"
                    :key="idx"
                    :src="image.image_url.url"
                    class="message-image"
                    :preview-src="image.image_url.url"
                  />
                </n-flex>
              </div>
              <div v-if="reasoning" class="reasoning-container">
                <n-collapse
                  arrow-placement="right"
                  display-directive="show"
                  v-model:expanded-names="thinkingPanelExpandedNames"
                >
                  <n-collapse-item title="Thinking" name="thinking">
                    <MarkdownRenderer
                      :text="reasoning"
                      :over="over"
                      v-model:ready="rendered"
                    />
                  </n-collapse-item>
                </n-collapse>
              </div>
              <MarkdownRenderer
                v-if="!hasToolCall"
                :text="displayText"
                :over="over"
                v-model:ready="rendered"
                @update:ready="onReadyStatusChange"
              />
              <template v-else>
                <MarkdownRenderer
                  v-if="textSegmentsBetweenTools.beforeFirst"
                  :text="textSegmentsBetweenTools.beforeFirst"
                  :over="over"
                  v-model:ready="rendered"
                  @update:ready="onReadyStatusChange"
                />
                <template v-for="(call, idx) in parsedToolCalls" :key="idx">
                  <div class="tool-call-container" :class="call.status">
                    <n-collapse
                      arrow-placement="right"
                      v-model:expanded-names="toolCallExpandedNames"
                      display-directive="show"
                    >
                      <n-collapse-item :name="`toolcall-${idx}`">
                        <template #header>
                          <n-flex align="center" :wrap="false" style="flex: 1;">
                            <n-icon :component="Toolbox24Regular" />
                            <n-tag size="small" :type="call.status === 'error' ? 'error' : 'success'">
                              {{ call.toolName }}
                            </n-tag>
                            <n-tag size="tiny" :type="call.status === 'error' ? 'error' : 'info'" round>
                              {{ call.status === 'error' ? 'Error' : 'OK' }}
                            </n-tag>
                            <span class="tool-call-hint">
                              {{ toolCallExpandedNames.includes(`toolcall-${idx}`) ? '点击收起' : '点击展开查看结果' }}
                            </span>
                          </n-flex>
                        </template>
                        <div class="tool-call-result">
                          <pre>{{ call.result }}</pre>
                        </div>
                      </n-collapse-item>
                    </n-collapse>
                  </div>
                  <MarkdownRenderer
                    v-if="idx < parsedToolCalls.length - 1 && textSegmentsBetweenTools.between[idx]"
                    :text="textSegmentsBetweenTools.between[idx]"
                    :over="over"
                    v-model:ready="rendered"
                    @update:ready="onReadyStatusChange"
                  />
                </template>
                <MarkdownRenderer
                  v-if="textSegmentsBetweenTools.afterLast"
                  :text="textSegmentsBetweenTools.afterLast"
                  :over="over"
                  v-model:ready="rendered"
                  @update:ready="onReadyStatusChange"
                />
              </template>
            </div>
          </div>
          <div
            class="footer"
            :style="{ visibility: footerVisible ? 'visible' : 'hidden' }"
          >
            <n-flex :wrap="false" align="center">
              <n-button-group class="button-group">
                <n-button quaternary :onclick="copyMessage" size="tiny">
                  <template #icon>
                    <n-icon :component="Copy16Regular" :size="16" />
                  </template>
                </n-button>
                <n-button
                  quaternary
                  :onclick="removeMessage"
                  type="error"
                  size="tiny"
                >
                  <template #icon>
                    <n-icon :component="Delete16Regular" :size="18" />
                  </template>
                </n-button>
                <n-button
                  quaternary
                  @click="emit('regenerate')"
                  size="tiny"
                  v-if="sender === 'bot'"
                >
                  <template #icon>
                    <n-icon :component="ArrowClockwise16Regular" :size="16" />
                  </template>
                </n-button>
                <n-button quaternary :onclick="() => emit('edit')" size="tiny">
                  <template #icon>
                    <n-icon :component="Edit16Regular" :size="16" />
                  </template>
                </n-button>
              </n-button-group>
              <n-button-group class="nav-group" v-if="hasPrevious || hasNext">
                <n-button
                  quaternary
                  @click="emit('previous')"
                  size="tiny"
                  :disabled="!hasPrevious"
                >
                  <template #icon>
                    <n-icon :component="ChevronLeft16Regular" :size="16" />
                  </template>
                </n-button>
                <n-button
                  quaternary
                  @click="emit('next')"
                  size="tiny"
                  :disabled="!hasNext"
                >
                  <template #icon>
                    <n-icon :component="ChevronRight16Regular" :size="16" />
                  </template>
                </n-button>
              </n-button-group>
            </n-flex>
            <span class="timestamp">{{ timestamp.toLocaleTimeString() }}</span>
          </div>
        </div>
      </n-flex>
    </div>
  </div>
</template>

<style scoped>
@keyframes fade-in {
  from {
    /* transform: scale(0.95); */
    opacity: 0;
  }
  to {
    /* transform: scale(1); */
    opacity: 1;
  }
}

.item-container {
  transform-origin: bottom 30%;
  --property-will-change: v-bind('isStreaming ? "height" : "auto"');
  will-change: var(--property-will-change);
  width: 100%;
  height: v-bind('(rendered || !culling ? "fit-content" : `${height}px`)');
  animation: fade-in 0.2s v-bind("theme.cubicBezierEaseIn");
}

.item-layout {
  flex-direction: v-bind(
    'sender === "user" ? "row-reverse" : "row"'
  ) !important;
  align-items: flex-start;
  margin-bottom: 12px;
}

.avatar {
  position: sticky;
  top: 8px;
  box-shadow: v-bind("theme.boxShadow2");
}

.message-bubble {
  max-width: 80%;
  will-change: var(--property-will-change);
  width: fit-content;

  display: grid;
  grid-template-columns: auto;
  grid-template-rows: auto, auto;
}

.message-bubble.user {
  color: white;
  margin-left: auto;
}

.message-bubble.bot {
  margin-right: auto;
}

.message-bubble.copied {
  transform-origin: bottom;
  animation: flash 0.5s ease;
}

@keyframes flash {
  0% {
    opacity: 1;
    transform: scale(1);
  }

  50% {
    opacity: 0.9;
    transform: scale(0.98);
  }

  100% {
    opacity: 1;
    transform: scale(1);
  }
}

.content-container {
  grid-area: 1 / 1 / 2 / 2;
  display: flex;
  justify-content: v-bind('sender === "bot" ? "flex-start" : "flex-end"');

  will-change: var(--property-will-change);
  min-width: 0;
  min-height: 0;
}

.content {
  width: fit-content;
  padding: 12px 16px;
  margin-left: 12px;
  margin-right: 12px;
  transition: all 0.2s ease;
  will-change: var(--property-will-change);

  background-color: v-bind("backgroundColor");
  border-radius: v-bind("theme.borderRadius");
  box-shadow: v-bind("theme.boxShadow2");
  border: v-bind("border");

  overflow: auto;
  display: flex;
  flex-direction: column;
  gap: 8px;
}

.reasoning-container {
  width: 100%;
  background-color: rgba(128, 128, 128, 0.2);
  padding: 8px 12px;
  box-sizing: border-box;
  border-radius: v-bind("theme.borderRadiusSmall");
  border: 1px solid v-bind("theme.borderColor");
  box-shadow: v-bind("theme.boxShadow3");
}

.images-container {
  width: 100%;
  margin-bottom: 8px;
}

.message-image {
  max-width: 200px;
  max-height: 200px;
  object-fit: cover;
  border-radius: v-bind("theme.borderRadiusSmall");
  cursor: pointer;
}

.tool-call-container {
  width: 100%;
  background-color: rgba(64, 160, 64, 0.1);
  padding: 8px 12px;
  box-sizing: border-box;
  border-radius: v-bind("theme.borderRadiusSmall");
  border: 1px solid rgba(64, 160, 64, 0.3);
  box-shadow: v-bind("theme.boxShadow3");
}

.tool-call-container.error {
  background-color: rgba(200, 60, 60, 0.1);
  border-color: rgba(200, 60, 60, 0.3);
}

.tool-call-hint {
  font-size: 0.85em;
  color: v-bind("theme.textColor3");
  margin-left: 8px;
}

.tool-call-result {
  background-color: rgba(0, 0, 0, 0.05);
  padding: 12px;
  border-radius: v-bind("theme.borderRadiusSmall");
  overflow-x: auto;
}

.tool-call-result pre {
  margin: 0;
  font-family: monospace;
  font-size: 0.9em;
  white-space: pre-wrap;
  word-break: break-all;
}

.footer {
  padding: 8px 16px 0 16px;
  width: 100%;
  box-sizing: border-box;
  min-width: fit-content;
  display: flex;
  justify-content: space-between;
  align-items: center;
  gap: 8px;
}

.placeholder {
  pointer-events: none;
  user-select: none;

  width: 100%;
  height: v-bind("`${height}px`");
}

.timestamp {
  font-size: 0.8em;
  margin-left: 16px;

  width: fit-content;
  font-family: monospace;
  color: v-bind("theme.textColorBase");
}
</style>
