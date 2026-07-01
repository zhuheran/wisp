import { describe, it, expect, beforeEach, vi } from 'vitest';
import { setActivePinia, createPinia } from 'pinia';
import { useCharacterStore } from '../../stores/character';

// Mock the commands module
vi.mock('../../libs/commands', () => ({
  configsGetCharacters: vi.fn().mockResolvedValue([]),
  configsCreateCharacter: vi.fn().mockResolvedValue(undefined),
  configsUpdateCharacter: vi.fn().mockResolvedValue(undefined),
  configsDeleteCharacter: vi.fn().mockResolvedValue(undefined),
  configsGetDefaultResponder: vi.fn().mockResolvedValue(null),
  configsSetDefaultResponder: vi.fn().mockResolvedValue(undefined),
}));

describe('characterStore', () => {
  beforeEach(() => {
    setActivePinia(createPinia());
  });

  it('createCharacter sets role_bio from input', async () => {
    const store = useCharacterStore();

    await store.createCharacter({
      name: 'Test Pal',
      alias: 'tester',
      description: 'A test character',
      system_prompt: 'You are a test bot.',
      parameters: [],
      model_id: 'gpt-4',
      role_bio: 'Expert in testing and quality assurance.',
    });

    expect(store.characters).toHaveLength(1);
    expect(store.characters[0].role_bio).toBe('Expert in testing and quality assurance.');
    expect(store.characters[0].name).toBe('Test Pal');
  });

  it('updateCharacter preserves role_bio when not changed', async () => {
    const store = useCharacterStore();

    // Create character with role_bio
    await store.createCharacter({
      name: 'Dev Pal',
      alias: 'dev',
      description: 'A developer character',
      system_prompt: 'You are a developer.',
      parameters: [],
      model_id: 'gpt-4',
      role_bio: 'Backend developer and system architect.',
    });

    const charId = store.characters[0].id;

    // Update only name and description (no role_bio included)
    await store.updateCharacter(charId, {
      name: 'Dev Pal Updated',
      description: 'An updated developer character',
    });

    // role_bio should still be preserved
    expect(store.characters[0].role_bio).toBe('Backend developer and system architect.');
    expect(store.characters[0].name).toBe('Dev Pal Updated');
  });

  it('updateCharacter can change role_bio when explicitly provided', async () => {
    const store = useCharacterStore();

    await store.createCharacter({
      name: 'Review Pal',
      alias: 'reviewer',
      description: 'A code reviewer',
      system_prompt: 'Review code.',
      parameters: [],
      model_id: 'gpt-4',
      role_bio: 'Code reviewer.',
    });

    const charId = store.characters[0].id;

    // Update role_bio
    await store.updateCharacter(charId, {
      role_bio: 'Senior code reviewer with Rust expertise.',
    });

    expect(store.characters[0].role_bio).toBe('Senior code reviewer with Rust expertise.');
  });
});
