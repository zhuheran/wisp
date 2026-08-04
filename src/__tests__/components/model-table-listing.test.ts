// @vitest-environment happy-dom
import { describe, it, expect, vi } from 'vitest'
import { mount } from '@vue/test-utils'
import { createPinia } from 'pinia'
import ModelTable from '../../components/ModelTable.vue'
import type { Provider } from '../../libs/types'

vi.mock('naive-ui', async (importOriginal) => {
  const actual = await importOriginal<typeof import('naive-ui')>()
  return {
    ...actual,
    useMessage: () => ({ info: vi.fn(), success: vi.fn(), error: vi.fn(), warning: vi.fn() }),
    useDialog: () => ({ warning: vi.fn() }),
  }
})

const openAiCompatibleProvider: Provider = {
  name: 'deepseek',
  display_name: 'DeepSeek',
  base_url: 'https://api.deepseek.com/',
  api_type: 'open_ai_compatible',
  models: [],
}

const groqProvider: Provider = {
  name: 'groq',
  display_name: 'Groq',
  base_url: '',
  api_type: 'groq',
  models: [],
}

function mountTable(provider: Provider) {
  return mount(ModelTable, {
    props: { provider },
    global: { plugins: [createPinia()] },
  })
}

// Regression: the toolbar lives in NCard's `#header-extra` slot, which NCard
// only renders when the card has a title or a `#header` slot. Without one the
// fetch button and the "Add models manually" hint are silently dropped.
describe('ModelTable listing control rendering', () => {
  it('renders the fetch button for open_ai_compatible providers', () => {
    const wrapper = mountTable(openAiCompatibleProvider)
    expect(wrapper.find('button[aria-label="Fetch available models"]').exists()).toBe(true)
    expect(wrapper.text()).not.toContain('Add models manually')
  })

  it('shows the manual hint for providers without a listing', () => {
    const wrapper = mountTable(groqProvider)
    expect(wrapper.text()).toContain('Add models manually')
    expect(wrapper.find('button[aria-label="Fetch available models"]').exists()).toBe(false)
  })
})
