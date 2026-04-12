import { INTERFACE_PROMPT } from '../constants/interfacePrompt'
import { usePromptStore } from '../stores/prompt'
import { useMcpStore } from '../../stores/mcp'

export const usePromptAssembly = () => {
  const promptStore = usePromptStore()
  const mcpStore = useMcpStore()

  const assembleMessages = (userMessage: string) => {
    const messages: Array<{ role: string; message: string }> = [
      {
        role: 'system',
        message: INTERFACE_PROMPT
      },
      {
        role: 'system',
        message: promptStore.selectedSystemPrompt
      }
    ]

    const toolsPrompt = mcpStore.getToolsPrompt()
    if (toolsPrompt) {
      messages.push({
        role: 'system',
        message: toolsPrompt
      })
    }

    messages.push({
      role: 'user',
      message: userMessage
    })

    return messages
  }

  return {
    assembleMessages,
    selectSystemPrompt: promptStore.selectSystemPrompt,
    currentSystemPromptId: promptStore.selectedSystemPromptId
  }
}
