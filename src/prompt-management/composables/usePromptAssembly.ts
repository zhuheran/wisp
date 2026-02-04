import { INTERFACE_PROMPT } from '../constants/interfacePrompt'
import { usePromptStore } from '../stores/prompt'

export const usePromptAssembly = () => {
  const promptStore = usePromptStore()

  const assembleMessages = (userMessage: string) => {
	return [
      {
        role: 'system',
        message: INTERFACE_PROMPT
      },
      {
        role: 'system',
        message: promptStore.selectedSystemPrompt
      },
      {
        role: 'user',
        message: userMessage
      }
    ]
  }

  return {
    assembleMessages,
    selectSystemPrompt: promptStore.selectSystemPrompt,
    currentSystemPromptId: promptStore.selectedSystemPromptId
  }
}
