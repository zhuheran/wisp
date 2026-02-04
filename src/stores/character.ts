import { defineStore } from 'pinia'
import { ref, computed } from 'vue'
import type { Character, CharacterParameter } from '../libs/types'
import {
  configsGetCharacters,
  configsCreateCharacter,
  configsUpdateCharacter,
  configsDeleteCharacter,
} from '../libs/commands'

export const useCharacterStore = defineStore('character', () => {
  const characters = ref<Character[]>([])
  const currentCharacterId = ref<string | null>(null)
  const isLoading = ref(false)

  const currentCharacter = computed(() => {
    if (!currentCharacterId.value) return null
    return characters.value.find(c => c.id === currentCharacterId.value) || null
  })

  const loadCharacters = async () => {
    isLoading.value = true
    try {
      const chars = await configsGetCharacters()
      characters.value = chars
      return chars
    } finally {
      isLoading.value = false
    }
  }

  const selectCharacter = (id: string | null) => {
    if (id === null) {
      currentCharacterId.value = null
      return
    }
    const character = characters.value.find(c => c.id === id)
    if (character) {
      currentCharacterId.value = id
    }
  }

  const createCharacter = async (character: Omit<Character, 'id' | 'created_at' | 'updated_at'>) => {
    isLoading.value = true
    try {
      const id = crypto.randomUUID()
      const now = Date.now()
      const newCharacter: Character = {
        ...character,
        id,
        created_at: now,
        updated_at: now,
      }
      await configsCreateCharacter(newCharacter)
      characters.value.push(newCharacter)
      return id
    } finally {
      isLoading.value = false
    }
  }

  const updateCharacter = async (id: string, data: Partial<Character>) => {
    isLoading.value = true
    try {
      const character = characters.value.find(c => c.id === id)
      if (!character) {
        throw new Error('Character not found')
      }
      const updatedCharacter = {
        ...character,
        ...data,
        updated_at: Date.now(),
      }
      await configsUpdateCharacter(id, updatedCharacter)
      const index = characters.value.findIndex(c => c.id === id)
      if (index !== -1) {
        characters.value[index] = updatedCharacter
      }
    } finally {
      isLoading.value = false
    }
  }

  const deleteCharacter = async (id: string) => {
    isLoading.value = true
    try {
      await configsDeleteCharacter(id)
      characters.value = characters.value.filter(c => c.id !== id)
      if (currentCharacterId.value === id) {
        currentCharacterId.value = null
      }
    } finally {
      isLoading.value = false
    }
  }

  const getParameterValue = (character: Character, name: string): any => {
    const param = character.parameters.find(p => p.name === name)
    return param?.value
  }

  const setParameterValue = async (characterId: string, name: string, value: any, metadata?: { label?: string; description?: string }) => {
    const character = characters.value.find(c => c.id === characterId)
    if (!character) return

    const existingIndex = character.parameters.findIndex(p => p.name === name)
    const parameters: CharacterParameter[] = [...character.parameters]

    if (existingIndex >= 0) {
      parameters[existingIndex] = {
        ...parameters[existingIndex],
        value,
        metadata: metadata || parameters[existingIndex].metadata,
      }
    } else {
      parameters.push({
        name,
        value,
        metadata: metadata || undefined,
      })
    }

    await updateCharacter(characterId, { parameters })
  }

  const removeParameter = async (characterId: string, name: string) => {
    const character = characters.value.find(c => c.id === characterId)
    if (!character) return

    const parameters = character.parameters.filter(p => p.name !== name)
    await updateCharacter(characterId, { parameters })
  }

  return {
    characters,
    currentCharacter,
    currentCharacterId,
    isLoading,
    loadCharacters,
    selectCharacter,
    createCharacter,
    updateCharacter,
    deleteCharacter,
    getParameterValue,
    setParameterValue,
    removeParameter,
  }
})
