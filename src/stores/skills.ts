import { defineStore } from 'pinia'
import { ref } from 'vue'
import type { SkillInfo } from '../libs/types'
import {
  skillsList,
  skillsRefresh,
  skillsToggle,
} from '../libs/commands'

export const useSkillsStore = defineStore('skills', () => {
  const skills = ref<SkillInfo[]>([])
  const isLoading = ref(false)

  const list = async () => {
    isLoading.value = true
    try {
      skills.value = await skillsList()
    } finally {
      isLoading.value = false
    }
  }

  const refresh = async () => {
    isLoading.value = true
    try {
      skills.value = await skillsRefresh()
    } finally {
      isLoading.value = false
    }
  }

  const toggle = async (name: string) => {
    // The backend returns the full updated list; assign it only on success so a
    // failed toggle leaves the UI unchanged.
    skills.value = await skillsToggle(name)
  }

  let initialized = false
  const init = async () => {
    if (initialized) return
    initialized = true
    await list()
  }

  return {
    skills,
    isLoading,
    init,
    list,
    refresh,
    toggle,
  }
})
