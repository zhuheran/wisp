<script setup lang="ts">
import { ref } from 'vue'
import { useThemeVars, useMessage, NCard } from 'naive-ui'
import { FolderOpen24Regular, ArrowSync24Regular } from '@vicons/fluent'
import { useSkillsStore } from '../stores/skills'
import { skillsOpenFolder } from '../libs/commands'
import type { SkillInfo } from '../libs/types'

const theme = useThemeVars()
const message = useMessage()
const skillsStore = useSkillsStore()

const openingFolder = ref(false)
const togglingName = ref<string | null>(null)

const handleOpenFolder = async () => {
  openingFolder.value = true
  try {
    await skillsOpenFolder()
  } catch (e) {
    message.error(`Failed to open skills folder: ${e}`)
  } finally {
    openingFolder.value = false
  }
}

const handleRefresh = async () => {
  try {
    await skillsStore.refresh()
    message.success('Skills rescanned')
  } catch (e) {
    message.error(`Failed to rescan skills: ${e}`)
  }
}

const handleToggle = async (skill: SkillInfo, enabled: boolean) => {
  togglingName.value = skill.name
  try {
    await skillsStore.toggle(skill.name)
    message.success(`${skill.name} ${enabled ? 'enabled' : 'disabled'}`)
  } catch (e) {
    message.error(`Failed to toggle skill "${skill.name}": ${e}`)
  } finally {
    togglingName.value = null
  }
}
</script>

<template>
  <div class="skills-view">
    <div class="header">
      <div class="header-text">
        <div class="header-title">Skills</div>
        <div class="header-subtitle">
          Agent skills (SKILL.md) installed in the skills folder
        </div>
      </div>
      <div class="header-actions">
        <n-button
          secondary
          :loading="openingFolder"
          @click="handleOpenFolder"
        >
          <template #icon>
            <n-icon><FolderOpen24Regular /></n-icon>
          </template>
          打开 skills 文件夹
        </n-button>
        <n-button
          type="primary"
          :loading="skillsStore.isLoading"
          @click="handleRefresh"
        >
          <template #icon>
            <n-icon><ArrowSync24Regular /></n-icon>
          </template>
          重新扫描
        </n-button>
      </div>
    </div>

    <div class="content">
      <n-spin :show="skillsStore.isLoading">
        <div v-if="skillsStore.skills.length > 0" class="skill-list">
          <n-card
            v-for="skill in skillsStore.skills"
            :key="skill.name"
            class="skill-card"
            :bordered="true"
          >
            <div class="skill-card-body">
               <div class="skill-info">
                <div class="skill-name" :title="skill.name">
                  {{ skill.name }}
                </div>
                <div class="skill-description" :title="skill.description">
                  {{ skill.description }}
                </div>
                <div class="skill-path" :title="skill.path">
                  {{ skill.path }}
                </div>
              </div>
              <div class="skill-toggle">
                <n-switch
                  :value="skill.enabled"
                  :loading="togglingName === skill.name"
                  :disabled="togglingName !== null && togglingName !== skill.name"
                  @update:value="(value: boolean) => handleToggle(skill, value)"
                />
              </div>
            </div>
          </n-card>
        </div>
        <n-empty
          v-else
          class="empty-state"
          description="将 SKILL.md 目录放入 skills 文件夹后点击重新扫描"
        />
      </n-spin>
    </div>
  </div>
</template>

<style scoped>
.skills-view {
  height: 100%;
  width: 100%;
  display: flex;
  flex-direction: column;
  box-sizing: border-box;
  gap: 16px;
}

.header {
  padding: 24px;
  padding-bottom: 0px;
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 16px;
  flex-shrink: 0;
}

.header-text {
  display: flex;
  flex-direction: column;
  gap: 4px;
  min-width: 0;
}

.header-title {
  font-size: 1.25em;
  font-weight: 600;
  color: v-bind('theme.textColor1');
}

.header-subtitle {
  font-size: 0.9em;
  color: v-bind('theme.textColor2');
}

.header-actions {
  display: flex;
  gap: 8px;
  flex-shrink: 0;
}

.content {
  flex-grow: 1;
  min-height: 0;
  overflow-y: auto;
}

.skill-list {
  display: flex;
  flex-direction: column;
  gap: 12px;
  padding-left: 16px;
  padding-right: 16px;
  padding-bottom: 12px;
}

.skill-card {
  border-radius: v-bind('theme.borderRadius');
  transition: 0.2s v-bind('theme.cubicBezierEaseInOut');
  box-shadow: v-bind('theme.boxShadow1');
}

.skill-card:hover {
  background-color: v-bind('theme.hoverColor');
}

.skill-card-body {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 16px;
}

.skill-info {
  flex-grow: 1;
  min-width: 0;
  display: flex;
  flex-direction: column;
  gap: 4px;
}

.skill-name {
  font-family: 'ui-monospace', 'SFMono-Regular', 'Menlo', 'Consolas', monospace;
  font-weight: 600;
  font-size: 1em;
  color: v-bind('theme.textColor1');
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}

.skill-description {
  font-size: 0.9em;
  color: v-bind('theme.textColor2');
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: wrap;
}

.skill-path {
  font-size: 0.8em;
  color: v-bind('theme.textColor3');
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}

.skill-toggle {
  flex-shrink: 0;
}

.empty-state {
  margin-top: 64px;
}
</style>
