import { listen } from '@tauri-apps/api/event'
import type { ConversationEventPayload } from '../libs/types'

export async function listenConversationEvents(
  handler: (event: ConversationEventPayload) => void,
) {
  return listen<ConversationEventPayload>('conversation_event', (event) => {
    handler(event.payload)
  })
}
