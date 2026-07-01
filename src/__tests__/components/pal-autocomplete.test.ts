// @vitest-environment happy-dom
import { describe, it, expect, vi } from "vitest";
import { mount } from "@vue/test-utils";
import PalAutocomplete from "../../components/PalAutocomplete.vue";

const mockCharacters = [
  { id: "1", name: "Coder", alias: "cod", description: "A coding pal", system_prompt: "", parameters: [], model_id: "gpt-4", created_at: 0, updated_at: 0, role_bio: "" },
  { id: "2", name: "Designer", alias: "dsg", description: "A design pal", system_prompt: "", parameters: [], model_id: "gpt-4", created_at: 0, updated_at: 0, role_bio: "" },
  { id: "3", name: "Writer", alias: "wrt", description: "A writing pal", system_prompt: "", parameters: [], model_id: "gpt-4", created_at: 0, updated_at: 0, role_bio: "" },
];

function createWrapper(modelValue = "", characters = mockCharacters) {
  return mount(PalAutocomplete, {
    props: {
      modelValue,
      characters,
    },
    attrs: {},
  });
}

describe("PalAutocomplete", () => {
  it("shows dropdown when @ is typed", async () => {
    const wrapper = createWrapper("@");
    await wrapper.vm.$nextTick();

    expect(wrapper.find(".pal-autocomplete-dropdown").exists()).toBe(true);
  });

  it("shows dropdown when @ is partially typed", async () => {
    const wrapper = createWrapper("Hey @cod");
    await wrapper.vm.$nextTick();

    expect(wrapper.find(".pal-autocomplete-dropdown").exists()).toBe(true);
  });

  it("hides dropdown when no @ is present", async () => {
    const wrapper = createWrapper("Hello world");
    await wrapper.vm.$nextTick();

    expect(wrapper.find(".pal-autocomplete-dropdown").exists()).toBe(false);
  });

  it("filters pals by typed text after @", async () => {
    const wrapper = createWrapper("@cod");
    await wrapper.vm.$nextTick();

    const items = wrapper.findAll(".pal-autocomplete-item");
    expect(items.length).toBe(1);
    expect(items[0].text()).toContain("Coder");
  });

  it("shows all pals when @ is empty", async () => {
    const wrapper = createWrapper("@");
    await wrapper.vm.$nextTick();

    const items = wrapper.findAll(".pal-autocomplete-item");
    expect(items.length).toBe(mockCharacters.length);
  });

  it("inserts mention on item click", async () => {
    const wrapper = createWrapper("Hello @cod");
    await wrapper.vm.$nextTick();

    const firstItem = wrapper.find(".pal-autocomplete-item");
    await firstItem.trigger("mousedown");

    const emitted = wrapper.emitted("update:modelValue");
    expect(emitted).toBeTruthy();
    if (emitted) {
      expect(emitted[0][0]).toBe("Hello @Coder ");
    }

    const mentionEmitted = wrapper.emitted("mention");
    expect(mentionEmitted).toBeTruthy();
    if (mentionEmitted) {
      expect(mentionEmitted[0]).toEqual(["1", "Coder"]);
    }
  });

  it("emits mention event with palId and palName", async () => {
    const wrapper = createWrapper("@dsg");
    await wrapper.vm.$nextTick();

    const items = wrapper.findAll(".pal-autocomplete-item");
    expect(items.length).toBe(1);

    await items[0].trigger("mousedown");

    const mentionEmitted = wrapper.emitted("mention");
    expect(mentionEmitted).toBeTruthy();
    if (mentionEmitted) {
      expect(mentionEmitted[0]).toEqual(["2", "Designer"]);
    }
  });

  it("dismisses dropdown on Escape", async () => {
    const wrapper = createWrapper("@");
    await wrapper.vm.$nextTick();

    expect(wrapper.find(".pal-autocomplete-dropdown").exists()).toBe(true);

    const event = new KeyboardEvent("keydown", { key: "Escape" });
    wrapper.find(".pal-autocomplete-wrapper").element.dispatchEvent(event);
    await wrapper.vm.$nextTick();

    expect(wrapper.find(".pal-autocomplete-dropdown").exists()).toBe(false);
  });

  it("navigates items with ArrowDown and ArrowUp", async () => {
    const wrapper = createWrapper("@");
    await wrapper.vm.$nextTick();

    // Initially first item should be active
    const items = wrapper.findAll(".pal-autocomplete-item");
    expect(items[0].classes()).toContain("pal-autocomplete-item--active");

    // Press ArrowDown
    const downEvent = new KeyboardEvent("keydown", { key: "ArrowDown" });
    wrapper.find(".pal-autocomplete-wrapper").element.dispatchEvent(downEvent);
    await wrapper.vm.$nextTick();

    expect(items[1].classes()).toContain("pal-autocomplete-item--active");

    // Press ArrowUp
    const upEvent = new KeyboardEvent("keydown", { key: "ArrowUp" });
    wrapper.find(".pal-autocomplete-wrapper").element.dispatchEvent(upEvent);
    await wrapper.vm.$nextTick();

    expect(items[0].classes()).toContain("pal-autocomplete-item--active");
  });

  it("selects item on Enter", async () => {
    const wrapper = createWrapper("@cod");
    await wrapper.vm.$nextTick();

    const enterEvent = new KeyboardEvent("keydown", { key: "Enter" });
    wrapper.find(".pal-autocomplete-wrapper").element.dispatchEvent(enterEvent);
    await wrapper.vm.$nextTick();

    const emitted = wrapper.emitted("update:modelValue");
    expect(emitted).toBeTruthy();
    if (emitted) {
      expect(emitted[0][0]).toBe("@Coder ");
    }
  });

  it("does not show dropdown for @ at start of a word (mid-word)", async () => {
    const wrapper = createWrapper("test@example");
    await wrapper.vm.$nextTick();

    // The @ is preceded by a word character, so it should not trigger
    expect(wrapper.find(".pal-autocomplete-dropdown").exists()).toBe(false);
  });

  it("shows dropdown when @ is at end with characters", async () => {
    const wrapper = createWrapper("Message @w");
    await wrapper.vm.$nextTick();

    expect(wrapper.find(".pal-autocomplete-dropdown").exists()).toBe(true);
    const items = wrapper.findAll(".pal-autocomplete-item");
    // Should match Writer (name contains 'w') and possibly others by alias
    const writer = items.filter((item) => item.text().includes("Writer"));
    expect(writer.length).toBeGreaterThan(0);
  });

  it("replaces @mention correctly when there is text after the search word", async () => {
    const wrapper = createWrapper("Hey @cod and then");
    await wrapper.vm.$nextTick();

    const firstItem = wrapper.find(".pal-autocomplete-item");
    await firstItem.trigger("mousedown");

    const emitted = wrapper.emitted("update:modelValue");
    expect(emitted).toBeTruthy();
    if (emitted) {
      expect(emitted[0][0]).toBe("Hey @Coder  and then");
    }
  });
});
