// @vitest-environment happy-dom
import { describe, it, expect, vi, beforeEach } from "vitest";
import { mount } from "@vue/test-utils";
import { createPinia, setActivePinia } from "pinia";
import { h, ref, nextTick } from "vue";
import { MessageRole } from "../../libs/types";
import MessageBubble from "../../components/MessageBubble.vue";
import { NCollapse } from "naive-ui";

vi.mock("@tauri-apps/plugin-clipboard-manager", () => ({
  writeText: vi.fn(),
}));

vi.mock("@tauri-apps/api/menu", () => ({
  Menu: { new: vi.fn(() => Promise.resolve({ popup: vi.fn() })) },
  MenuItem: { new: vi.fn() },
  PredefinedMenuItem: { new: vi.fn() },
}));

vi.mock("../../components/MarkdownRenderer.vue", () => ({
  default: {
    name: "MarkdownRenderer",
    props: { text: String, over: Boolean, modelValue: Boolean, "onUpdate:modelValue": Function },
    template: "<div class='mock-markdown'>{{ text }}</div>",
  },
}));

// Mock naive-ui composables that require provider context
vi.mock("naive-ui", async (importOriginal) => {
  const actual: Record<string, unknown> = await importOriginal();
  return {
    ...actual,
    useDialog: () => {
      const warning = vi.fn();
      return { warning, info: vi.fn(), error: vi.fn(), success: vi.fn() };
    },
    useThemeVars: () =>
      ref({
        borderColor: "#ddd",
        primaryColor: "#18a058",
        baseColor: "#fff",
        actionColor: "#f5f5f5",
        cardColor: "#fff",
        borderRadius: "8px",
        borderRadiusSmall: "4px",
        boxShadow2: "0 1px 2px rgba(0,0,0,0.06)",
        boxShadow3: "0 1px 4px rgba(0,0,0,0.08)",
        textColor3: "#999",
        textColorBase: "#333",
      }),
  };
});

const mockCharacterStore = {
  characters: [],
  loadCharacters: vi.fn(),
};

function createWrapper(props: Record<string, unknown> = {}) {
  const pinia = createPinia();

  return mount(MessageBubble, {
    props: {
      id: "test-1",
      text: "Hello world",
      sender: MessageRole.Assistant,
      timestamp: new Date("2025-06-01T12:00:00Z"),
      ...props,
    },
    global: {
      plugins: [pinia],
      provide: {
        CharacterStore: mockCharacterStore,
      },
    },
    attachTo: document.body,
  });
}

describe("MessageBubble pal display", () => {
  beforeEach(() => {
    setActivePinia(createPinia());
  });

  it("shows pal name and icon when pal_id is set", async () => {
    const wrapper = createWrapper({
      pal_id: "pal-1",
      pal_name: "Coder",
      source: "directed",
    });

    const header = wrapper.find(".message-pal-header");
    expect(header.exists()).toBe(true);
    expect(header.text()).toContain("Coder");
  });

  it('shows "directed" badge when source is directed', async () => {
    const wrapper = createWrapper({
      pal_id: "pal-1",
      pal_name: "Coder",
      source: "directed",
    });

    const header = wrapper.find(".message-pal-header");
    expect(header.text()).toContain("🎬");
    expect(header.text()).toContain("directed");
  });

  it('shows "mentioned" badge when source is user_prompted', async () => {
    const wrapper = createWrapper({
      pal_id: "pal-1",
      pal_name: "Coder",
      source: "user_prompted",
    });

    const header = wrapper.find(".message-pal-header");
    expect(header.text()).toContain("📍");
    expect(header.text()).toContain("mentioned");
  });

  it("hides pal section when pal_id is absent", async () => {
    const wrapper = createWrapper({
      pal_id: undefined,
      pal_name: undefined,
      source: undefined,
    });

    const header = wrapper.find(".message-pal-header");
    expect(header.exists()).toBe(false);
  });

  it("hides pal section when pal_id is absent regardless of source", async () => {
    const wrapper = createWrapper({
      pal_id: undefined,
      pal_name: undefined,
      source: "directed",
    });

    const header = wrapper.find(".message-pal-header");
    expect(header.exists()).toBe(false);
  });

  it("omits badge when source is not set even if pal_id is set", async () => {
    const wrapper = createWrapper({
      pal_id: "pal-1",
      pal_name: "Coder",
      source: undefined,
    });

    const header = wrapper.find(".message-pal-header");
    expect(header.exists()).toBe(true);
    expect(header.text()).toContain("Coder");
    expect(header.text()).not.toContain("🎬");
    expect(header.text()).not.toContain("📍");
    expect(header.text()).not.toContain("directed");
    expect(header.text()).not.toContain("mentioned");
  });
});

describe("MessageBubble thinking panel expand/collapse", () => {
  beforeEach(() => {
    setActivePinia(createPinia());
  });

  it("expands thinking panel while streaming (over=false)", async () => {
    const wrapper = createWrapper({
      reasoning: "let me think...",
      over: false,
    });

    const reasoningContainer = wrapper.find(".reasoning-container");
    const collapse = reasoningContainer.findComponent(NCollapse);
    expect(collapse.props("expandedNames")).toEqual(["thinking"]);
  });

  it("collapses thinking panel when over=true", async () => {
    const wrapper = createWrapper({
      reasoning: "let me think...",
      over: true,
    });

    const reasoningContainer = wrapper.find(".reasoning-container");
    const collapse = reasoningContainer.findComponent(NCollapse);
    expect(collapse.props("expandedNames")).toEqual([]);
  });

  it("auto-collapses when streaming transitions to over=true", async () => {
    const wrapper = createWrapper({
      reasoning: "let me think...",
      over: false,
    });

    let collapse = wrapper.find(".reasoning-container").findComponent(NCollapse);
    expect(collapse.props("expandedNames")).toEqual(["thinking"]);

    await wrapper.setProps({ over: true });
    await nextTick();
    collapse = wrapper.find(".reasoning-container").findComponent(NCollapse);
    expect(collapse.props("expandedNames")).toEqual([]);
  });

  it("allows manual expand after streaming ends (v-model is writable)", async () => {
    const wrapper = createWrapper({
      reasoning: "let me think...",
      over: true,
    });

    const reasoningContainer = wrapper.find(".reasoning-container");
    const collapse = reasoningContainer.findComponent(NCollapse);
    expect(collapse.props("expandedNames")).toEqual([]);

    // Simulate naive-ui emitting update:expandedNames when user clicks the header.
    // A read-only computed (the previous bug) would throw here.
    collapse.vm.$emit("update:expandedNames", ["thinking"]);
    await nextTick();

    expect(collapse.props("expandedNames")).toEqual(["thinking"]);
  });

  it("supports manual expand for grouped messages", async () => {
    const wrapper = createWrapper({
      groupMessages: [
        { text: "first", reasoning: "thinking 1", sender: MessageRole.Assistant },
        { text: "second", reasoning: "thinking 2", sender: MessageRole.Assistant },
      ],
      text: "",
      over: true,
    });

    const collapses = wrapper.findAllComponents(NCollapse);
    const reasoningCollapses = collapses.filter((c) =>
      c.props("expandedNames") !== undefined
    );
    expect(reasoningCollapses.length).toBeGreaterThanOrEqual(2);

    const first = reasoningCollapses[0];
    expect(first.props("expandedNames")).toEqual([]);

    first.vm.$emit("update:expandedNames", ["thinking-0"]);
    await nextTick();
    expect(first.props("expandedNames")).toEqual(["thinking-0"]);
  });
});
