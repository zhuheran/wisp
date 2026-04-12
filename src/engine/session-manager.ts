import type { SessionState, SessionStore, ConversationMessage } from './types'

const STORAGE_PREFIX = 'wisp_session_'

export class InMemorySessionStore implements SessionStore {
  private sessions = new Map<string, SessionState>()

  async save(session: SessionState): Promise<void> {
    this.sessions.set(session.id, { ...session, updatedAt: Date.now() })
  }

  async load(id: string): Promise<SessionState | null> {
    return this.sessions.get(id) ?? null
  }

  async delete(id: string): Promise<void> {
    this.sessions.delete(id)
  }

  async list(): Promise<SessionState[]> {
    return Array.from(this.sessions.values())
  }
}

export class LocalStorageSessionStore implements SessionStore {
  async save(session: SessionState): Promise<void> {
    const key = `${STORAGE_PREFIX}${session.id}`
    const data = JSON.stringify({ ...session, updatedAt: Date.now() })
    localStorage.setItem(key, data)
  }

  async load(id: string): Promise<SessionState | null> {
    const key = `${STORAGE_PREFIX}${id}`
    const data = localStorage.getItem(key)
    if (!data) return null
    try {
      return JSON.parse(data) as SessionState
    } catch {
      return null
    }
  }

  async delete(id: string): Promise<void> {
    const key = `${STORAGE_PREFIX}${id}`
    localStorage.removeItem(key)
  }

  async list(): Promise<SessionState[]> {
    const sessions: SessionState[] = []
    for (let i = 0; i < localStorage.length; i++) {
      const key = localStorage.key(i)
      if (key?.startsWith(STORAGE_PREFIX)) {
        const data = localStorage.getItem(key)
        if (data) {
          try {
            sessions.push(JSON.parse(data))
          } catch {
            // skip corrupt entries
          }
        }
      }
    }
    return sessions
  }
}

export class SessionManager {
  private store: SessionStore
  private activeSessionId: string | null = null

  constructor(store?: SessionStore) {
    this.store = store ?? new InMemorySessionStore()
  }

  async createSession(systemPrompt?: string): Promise<SessionState> {
    const session: SessionState = {
      id: crypto.randomUUID(),
      messages: systemPrompt
        ? [{ role: 'system', content: systemPrompt }]
        : [],
      createdAt: Date.now(),
      updatedAt: Date.now(),
      metadata: {},
    }
    await this.store.save(session)
    this.activeSessionId = session.id
    return session
  }

  async getSession(): Promise<SessionState | null> {
    if (!this.activeSessionId) return null
    return this.store.load(this.activeSessionId)
  }

  async appendMessage(message: ConversationMessage): Promise<void> {
    const session = await this.getSession()
    if (!session) throw new Error('No active session')
    session.messages.push(message)
    await this.store.save(session)
  }

  async appendMessages(messages: ConversationMessage[]): Promise<void> {
    const session = await this.getSession()
    if (!session) throw new Error('No active session')
    session.messages.push(...messages)
    await this.store.save(session)
  }

  async switchSession(id: string): Promise<SessionState | null> {
    const session = await this.store.load(id)
    if (session) {
      this.activeSessionId = id
    }
    return session
  }

  async deleteSession(id: string): Promise<void> {
    await this.store.delete(id)
    if (this.activeSessionId === id) {
      this.activeSessionId = null
    }
  }

  async listSessions(): Promise<SessionState[]> {
    return this.store.list()
  }

  getActiveSessionId(): string | null {
    return this.activeSessionId
  }

  async updateMetadata(key: string, value: unknown): Promise<void> {
    const session = await this.getSession()
    if (!session) throw new Error('No active session')
    session.metadata[key] = value
    await this.store.save(session)
  }
}
