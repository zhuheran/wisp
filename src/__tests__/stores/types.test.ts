import { describe, it, expect } from 'vitest'
import { MessageRole, type Message, type Character, type Conversation, type ConversationSendRequest } from '../../libs/types'

describe('Message source type', () => {
  it('accepts user_prompted and directed values', () => {
    const m1: Message = {
      id: '1',
      text: 'hello',
      sender: MessageRole.User,
      timestamp: 1,
      source: 'user_prompted',
    }
    const m2: Message = {
      id: '2',
      text: 'world',
      sender: MessageRole.Assistant,
      timestamp: 2,
      source: 'directed',
    }
    expect(m1.source).toBe('user_prompted')
    expect(m2.source).toBe('directed')
  })
})

describe('Message pal fields', () => {
  it('allows optional pal_id and pal_name', () => {
    const withPal: Message = {
      id: '1',
      text: 'hello',
      sender: MessageRole.User,
      timestamp: 1,
      source: 'user_prompted',
      pal_id: 'pal-123',
      pal_name: 'Test Pal',
    }
    const withoutPal: Message = {
      id: '2',
      text: 'world',
      sender: MessageRole.User,
      timestamp: 2,
      source: 'directed',
    }
    expect(withPal.pal_id).toBe('pal-123')
    expect(withPal.pal_name).toBe('Test Pal')
    expect(withoutPal.pal_id).toBeUndefined()
    expect(withoutPal.pal_name).toBeUndefined()
  })
})

describe('Character role_bio field', () => {
  it('allows optional role_bio', () => {
    const withBio: Character = {
      id: 'char-1',
      name: 'Test Character',
      description: 'A test character',
      system_prompt: 'You are a test',
      parameters: [],
      model_id: 'model-1',
      created_at: 1000,
      updated_at: 1000,
      role_bio: 'A helpful assistant',
    }
    const withoutBio: Character = {
      id: 'char-2',
      name: 'Minimal Character',
      description: 'No bio needed',
      system_prompt: 'Just respond',
      parameters: [],
      model_id: 'model-2',
      created_at: 2000,
      updated_at: 2000,
    }
    expect(withBio.role_bio).toBe('A helpful assistant')
    expect(withoutBio.role_bio).toBeUndefined()
  })
})

describe('Conversation default_pal_id field', () => {
  it('allows optional default_pal_id', () => {
    const withPal: Conversation = {
      id: 'conv-1',
      name: 'Test Conversation',
      default_pal_id: 'pal-123',
    }
    const withoutPal: Conversation = {
      id: 'conv-2',
      name: 'Minimal Conversation',
    }
    expect(withPal.default_pal_id).toBe('pal-123')
    expect(withoutPal.default_pal_id).toBeUndefined()
  })
})

describe('ConversationSendRequest target_pal_ids field', () => {
  it('allows optional target_pal_ids', () => {
    const withTargets: ConversationSendRequest = {
      conversation_id: 'conv-1',
      text: 'hello',
      model: 'model-1',
      provider: {
        name: 'test',
        display_name: 'Test',
        base_url: 'http://localhost',
        models: [],
      },
      target_pal_ids: ['pal-1', 'pal-2'],
    }
    const withoutTargets: ConversationSendRequest = {
      conversation_id: 'conv-2',
      text: 'world',
      model: 'model-2',
      provider: {
        name: 'test',
        display_name: 'Test',
        base_url: 'http://localhost',
        models: [],
      },
    }
    expect(withTargets.target_pal_ids).toEqual(['pal-1', 'pal-2'])
    expect(withoutTargets.target_pal_ids).toBeUndefined()
  })
})
