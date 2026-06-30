use super::conversations::Conversations;
use super::messages::Messages;
use super::threads::Threads;
use super::types::{ChatError, Conversation, ConversationError, Message, ThreadTreeItem};
use super::{create_pool, DbPool};
use std::path::PathBuf;
use tauri::{AppHandle, Manager};

pub struct Chat {
    pool: DbPool,
    pub thread_manager: Threads,
    pub conversation_manager: Conversations,
    pub messages_manager: Messages,
}

#[allow(unused)]
impl Chat {
    pub fn new_with_pool(pool: DbPool) -> Result<Self, ChatError> {
        let messages_manager = Messages::new(pool.clone())?;
        let thread_manager = Threads::new(pool.clone(), "messages", "id")?;
        let conversation_manager = Conversations::new(pool.clone(), "messages")?;

        Ok(Chat {
            pool,
            thread_manager,
            conversation_manager,
            messages_manager,
        })
    }

    pub fn new(app_handle: &AppHandle) -> Result<Self, ChatError> {
        let app_dir = app_handle
            .path()
            .app_data_dir()
            .expect("Failed to get app data dir");
        println!("App dir: {:?}", app_dir);
        std::fs::create_dir_all(&app_dir).expect("Failed to create app data dir");
        let db_path = PathBuf::from(app_dir).join("messages.db");
        let db_path = db_path.to_str().expect("Failed to reach database path");

        let pool = create_pool(db_path);
        Self::new_with_pool(pool)
    }

    /// Creates a new conversation with initial system message
    pub fn create_conversation(
        &mut self,
        conversation_id: &str,
        name: &str,
        description: &str,
    ) -> Result<(), ChatError> {
        let mut conn = self.pool.get()?;
        let tx = conn.transaction()?;

        self.conversation_manager
            .create(conversation_id, name, Some(description), None)?;

        tx.commit()?;
        Ok(())
    }

    /// Adds a message to an existing conversation thread
    pub fn add_message(
        &mut self,
        conversation_id: &str,
        message_id: &str,
        text: &str,
			reasoning: Option<&str>,
        sender: &str,
        parent_message_id: Option<&str>,
        images: Option<&str>,
        tool_calls: Option<&str>,
    ) -> Result<(), ChatError> {
        let mut conn = self.pool.get()?;
        let tx = conn.transaction()?;

        // Add the message
        self.messages_manager
            .add(message_id, text, reasoning, sender, None, None, images, tool_calls)?;

        // Link to parent message
        self.thread_manager.add(message_id, parent_message_id)?;

        // Link to conversation's entry message if no parent specified
        if parent_message_id.is_none() {
            let conv =
                self.conversation_manager
                    .get(conversation_id)?
                    .ok_or(ChatError::Conversation(ConversationError::Database(
                        rusqlite::Error::QueryReturnedNoRows,
                    )))?;
            let conv_id: &str = &conv.id;
            self.conversation_manager
                .update_entry_message_id(conv_id, Some(message_id))?;
        }

        tx.commit()?;
        Ok(())
    }

    /// Gets the selected branch path from the conversation root to a leaf message.
    pub fn get_message_path_to(
        &mut self,
        conversation_id: &str,
        leaf_message_id: &str,
    ) -> Result<Vec<Message>, ChatError> {
        let conv = self
            .conversation_manager
            .get(conversation_id)?
            .ok_or(ChatError::Conversation(ConversationError::Database(
                rusqlite::Error::QueryReturnedNoRows,
            )))?;

        let Some(entry_id) = conv.entry_message_id else {
            return Ok(vec![]);
        };

        let mut ids = Vec::new();
        let mut current = Some(leaf_message_id.to_string());
        while let Some(id) = current {
            ids.push(id.clone());
            if id == entry_id {
                break;
            }
            current = self.thread_manager.get_parent(&id)?;
        }

        if ids.last() != Some(&entry_id) {
            return Err(ChatError::Conversation(ConversationError::InvalidOperation(
                format!("message {leaf_message_id} is not in conversation {conversation_id}"),
            )));
        }

        ids.reverse();
        ids.into_iter()
            .map(|id| self.messages_manager.get(&id).map_err(ChatError::from))
            .collect()
    }

    /// Gets the default leaf by always selecting the last child at each branch.
    pub fn get_default_leaf(&mut self, conversation_id: &str) -> Result<Option<String>, ChatError> {
        let conv = self
            .conversation_manager
            .get(conversation_id)?
            .ok_or(ChatError::Conversation(ConversationError::Database(
                rusqlite::Error::QueryReturnedNoRows,
            )))?;

        let Some(mut current) = conv.entry_message_id else {
            return Ok(None);
        };

        loop {
            let children = self.thread_manager.get_children(&current)?;
            let Some(next) = children.last() else {
                return Ok(Some(current));
            };
            current = next.clone();
        }
    }

    /// Gets full message thread for a conversation
    pub fn get_all_message_involved(
        &mut self,
        conversation_id: &str,
    ) -> Result<Vec<Message>, ChatError> {
        let conv =
            self.conversation_manager
                .get(conversation_id)?
                .ok_or(ChatError::Conversation(ConversationError::Database(
                    rusqlite::Error::QueryReturnedNoRows,
                )))?;

        if conv.entry_message_id.is_none() {
            return Ok(vec![]);
        }

        let entry_id = conv
            .entry_message_id
            .as_deref()
            .ok_or(ChatError::Conversation(ConversationError::Database(
                rusqlite::Error::InvalidQuery,
            )))?;

        // Start with the entry message
        let mut messages = vec![self.messages_manager.get(entry_id)?];

        // Recursively get all threaded messages
        let mut current_level = vec![entry_id.to_string()];
        while !current_level.is_empty() {
            let mut next_level = Vec::new();
            for parent_id in &current_level {
                let children = self.thread_manager.get_children(parent_id)?;
                for child_id in children {
                    messages.push(self.messages_manager.get(&child_id)?);
                    next_level.push(child_id);
                }
            }
            current_level = next_level;
        }

        Ok(messages)
    }

    /// Deletes a conversation and all its messages
    pub fn delete_conversation(&mut self, conversation_id: &str) -> Result<(), ChatError> {
        let mut conn = self.pool.get()?;
        let tx = conn.transaction()?;

        // 即使获取消息列表失败也尝试删除会话本身，但记录错误以便排查。
        // 注意：由于子管理器使用独立连接，此处的 tx 仅保护本语句。
        match self.get_all_message_involved(conversation_id) {
            Ok(messages) => {
                for message in messages {
                    self.messages_manager.delete(&message.id)?;
                    self.thread_manager.delete_with_parent(&message.id)?;
                }
            }
            Err(e) => {
                eprintln!("[Chat] Failed to list messages while deleting conversation {}: {:?}. Proceeding to delete conversation record only; orphan messages may remain.", conversation_id, e);
            }
        }

        self.conversation_manager.delete(conversation_id)?;

        tx.commit()?;
        Ok(())
    }

    /// Lists all conversations with their names
    pub fn list_conversations(&mut self) -> Result<Vec<Conversation>, ChatError> {
        let convs = self.conversation_manager.list()?;
        Ok(convs)
    }

    /// Updates a message's content
    pub fn update_message(&mut self, message_id: &str, new_text: &str) -> Result<(), ChatError> {
        self.messages_manager.update_text(message_id, new_text)?;
        Ok(())
    }

    /// Deletes a message and its thread relationships, returns the new parent message ID if any.
    pub fn delete_message(
        &mut self,
        message_id: &str,
        recursive: bool,
    ) -> Result<Option<String>, ChatError> {
        let mut conn = self.pool.get()?;
        let tx = conn.transaction()?;

        if recursive {
            // get all children of the message recursively
            let mut all_children = Vec::new();
            let mut current_children = self.thread_manager.get_children(message_id)?;
            while !current_children.is_empty() {
                let mut next_children = Vec::new();
                for child in &current_children {
                    next_children.extend(self.thread_manager.get_children(&child)?);
                }
                all_children.extend(current_children);
                current_children = next_children;
            }

            // Delete all children
            for child_id in all_children {
                self.messages_manager.delete(&child_id)?;
            }

            let parent = self.thread_manager.get_parent(message_id)?;
            if (parent.is_none()) {
                let conversation = self
                    .conversation_manager
                    .get_by_entry_id(message_id)?
                    .ok_or(ChatError::Conversation(ConversationError::Database(
                        rusqlite::Error::QueryReturnedNoRows,
                    )))?;
                self.conversation_manager
                    .update_entry_message_id(&conversation.id, None)?
            }

            // Delete the original message
            self.messages_manager.delete(message_id)?;

            tx.commit()?;
            Ok(None)
        } else {
            let parent = self.thread_manager.get_parent(message_id)?;
            let children = self.thread_manager.get_children(message_id)?;

            match &parent {
                Some(p) => {
                    for child in &children {
                        self.thread_manager.update_parent(child, Some(p))?;
                    }
                }
                None => {
                    // root message
                    match children.len() {
                        0 => {
                            let conversation = self
                                .conversation_manager
                                .get_by_entry_id(message_id)?
                                .ok_or(ChatError::Conversation(ConversationError::Database(
                                    rusqlite::Error::QueryReturnedNoRows,
                                )))?;
                            self.conversation_manager
                                .update_entry_message_id(&conversation.id, None)?
                        }
                        1 => {
                            let conversation = self
                                .conversation_manager
                                .get_by_entry_id(message_id)?
                                .ok_or(ChatError::Conversation(ConversationError::Database(
                                    rusqlite::Error::QueryReturnedNoRows,
                                )))?;
                            self.conversation_manager
                                .update_entry_message_id(&conversation.id, Some(&children[0]))?
                        }
                        _ => {
                            return Err(ChatError::Conversation(
                                ConversationError::InvalidOperation(
                                    "Cannot delete root message with children".to_string(),
                                ),
                            ));
                        }
                    }
                }
            };

            // Delete message
            self.messages_manager.delete(message_id)?;
            tx.commit()?;
            Ok(parent)
        }
    }

    /// Builds a tree structure of messages starting from the entry message
    pub fn get_thread_tree(
        &mut self,
        conversation_id: &str,
    ) -> Result<Vec<ThreadTreeItem>, ChatError> {
        let conv =
            self.conversation_manager
                .get(conversation_id)?
                .ok_or(ChatError::Conversation(ConversationError::Database(
                    rusqlite::Error::QueryReturnedNoRows,
                )))?;

        // If there's no entry message, return an empty tree
        let Some(entry_id) = conv.entry_message_id else {
            return Ok(vec![]);
        };

        let mut result: Vec<ThreadTreeItem> = vec![];
        let mut stack = vec![entry_id.clone()];

        while let Some(message_id) = stack.pop() {
            let children_ids = self.thread_manager.get_children(&message_id)?;
            let parent_id = self.thread_manager.get_parent(&message_id)?;
            result.push(ThreadTreeItem {
                key: message_id.clone(),
                parent: parent_id,
                children: children_ids.clone(),
            });
            stack.extend(children_ids);
        }

        Ok(result)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::create_memory_pool;

    fn add(chat: &mut Chat, conversation_id: &str, id: &str, text: &str, sender: &str, parent: Option<&str>) {
        chat.add_message(conversation_id, id, text, None, sender, parent, None, None)
            .expect("message inserted");
    }

    #[test]
    fn get_message_path_to_returns_only_selected_branch() {
        let pool = create_memory_pool();
        let mut chat = Chat::new_with_pool(pool).expect("chat created");
        chat.create_conversation("c1", "Conversation", "desc")
            .expect("conversation created");

        add(&mut chat, "c1", "u1", "root user", "user", None);
        add(&mut chat, "c1", "a1", "assistant branch 1", "bot", Some("u1"));
        add(&mut chat, "c1", "a2", "assistant branch 2", "bot", Some("u1"));
        add(&mut chat, "c1", "u2", "follow up", "user", Some("a2"));

        let path = chat
            .get_message_path_to("c1", "u2")
            .expect("path selected");
        let ids: Vec<String> = path.into_iter().map(|message| message.id).collect();

        assert_eq!(ids, vec!["u1", "a2", "u2"]);
    }

    #[test]
    fn get_default_leaf_selects_last_child_recursively() {
        let pool = create_memory_pool();
        let mut chat = Chat::new_with_pool(pool).expect("chat created");
        chat.create_conversation("c1", "Conversation", "desc")
            .expect("conversation created");

        add(&mut chat, "c1", "u1", "root user", "user", None);
        add(&mut chat, "c1", "a1", "assistant branch 1", "bot", Some("u1"));
        add(&mut chat, "c1", "a2", "assistant branch 2", "bot", Some("u1"));
        add(&mut chat, "c1", "u2", "follow up", "user", Some("a2"));

        assert_eq!(
            chat.get_default_leaf("c1").expect("leaf selected"),
            Some("u2".to_string())
        );
    }

    #[test]
    fn get_message_path_to_rejects_leaf_outside_conversation() {
        let pool = create_memory_pool();
        let mut chat = Chat::new_with_pool(pool).expect("chat created");
        chat.create_conversation("c1", "Conversation 1", "desc")
            .expect("conversation created");
        chat.create_conversation("c2", "Conversation 2", "desc")
            .expect("conversation created");

        add(&mut chat, "c1", "u1", "root user", "user", None);
        add(&mut chat, "c2", "u2", "other root", "user", None);

        let err = chat
            .get_message_path_to("c1", "u2")
            .expect_err("foreign leaf rejected");
        assert!(matches!(
            err,
            ChatError::Conversation(ConversationError::InvalidOperation(_))
        ));
    }
}
