// @vitest-environment happy-dom
import { describe, it, expect } from "vitest";
import { mount } from "@vue/test-utils";
import { createPinia, setActivePinia } from "pinia";
import { useCharacterStore } from "../../stores/character";
import ChatPalBar from "../../components/ChatPalBar.vue";

const mockCharacters = [
  {
    id: "1",
    name: "Coder",
    alias: "cod",
    description: "A coding pal",
    system_prompt: "",
    parameters: [],
    model_id: "gpt-4",
    created_at: 0,
    updated_at: 0,
    role_bio: "",
  },
  {
    id: "2",
    name: "PM",
    alias: "pm",
    description: "A product pal",
    system_prompt: "",
    parameters: [],
    model_id: "gpt-4",
    created_at: 0,
    updated_at: 0,
    role_bio: "",
  },
  {
    id: "3",
    name: "Designer",
    alias: "dsg",
    description: "A design pal",
    system_prompt: "",
    parameters: [],
    model_id: "gpt-4",
    created_at: 0,
    updated_at: 0,
    role_bio: "",
  },
];

function createWrapper(palIds: string[]) {
  setActivePinia(createPinia());
  const store = useCharacterStore();
  store.characters = mockCharacters;
  return mount(ChatPalBar, {
    props: { palIds },
  });
}

describe("ChatPalBar", () => {
  it("shows avatar icons for each member", () => {
    const wrapper = createWrapper(["1", "2"]);
    const avatars = wrapper.findAll(".pal-avatar");
    expect(avatars.length).toBe(2);
  });

  it("shows pal name on hover", async () => {
    const wrapper = createWrapper(["1"]);
    const avatar = wrapper.find(".pal-avatar");
    expect(avatar.attributes("title")).toContain("Coder");
  });

  it("shows empty state when no pals", () => {
    const wrapper = createWrapper([]);
    expect(wrapper.find(".pal-bar-empty").exists()).toBe(true);
  });

  it("skips unknown pal IDs gracefully", () => {
    const wrapper = createWrapper(["1", "nonexistent"]);
    const avatars = wrapper.findAll(".pal-avatar");
    // Only "1" resolves to a known character
    expect(avatars.length).toBe(1);
    expect(avatars[0].attributes("title")).toContain("Coder");
  });
});
