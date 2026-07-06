use crate::pool::DbPool;
use crate::types::{Conversation, ConversationError};
use rusqlite::params;

pub struct Conversations {
    pool: DbPool,
}

#[allow(unused)]
impl Conversations {
    pub const TABLE_NAME: &'static str = "conversations";

    pub fn new(pool: DbPool, message_table_name: &str) -> Result<Self, ConversationError> {
        let conn = pool.get().map_err(ConversationError::Pool)?;

        conn.execute(
            &format!(
                "CREATE TABLE IF NOT EXISTS {} (
						id TEXT PRIMARY KEY,
						name TEXT NOT NULL,
						description TEXT,
						entry_message_id TEXT,
						default_pal_id TEXT,
						FOREIGN KEY (entry_message_id) REFERENCES {} (id) ON DELETE CASCADE
					)",
                Self::TABLE_NAME,
                message_table_name
            ),
            [],
        )?;

        // Migration: add default_pal_id column for databases created before this column existed
        let _ = conn.execute(
            &format!("ALTER TABLE {} ADD COLUMN default_pal_id TEXT", Self::TABLE_NAME),
            [],
        );

        // Migration: add thread_decisions column (JSON-encoded positional
        // child indices recording the user's selected branch path). Stored
        // alongside messages so the selection survives reloads.
        let _ = conn.execute(
            &format!("ALTER TABLE {} ADD COLUMN thread_decisions TEXT", Self::TABLE_NAME),
            [],
        );

        Ok(Self { pool })
    }

    pub fn create(
        &mut self,
        id: &str,
        name: &str,
        description: Option<&str>,
        entry_message_id: Option<&str>,
        default_pal_id: Option<&str>,
    ) -> Result<(), ConversationError> {
        let conn = self.pool.get()?;
        conn.execute(
				&format!(
					"INSERT INTO {} (id, name, description, entry_message_id, default_pal_id) VALUES (?1, ?2, ?3, ?4, ?5)",
					Self::TABLE_NAME
				),
				params![id, name, description, entry_message_id, default_pal_id],
			)?;
        Ok(())
    }

    pub fn exists(&mut self, id: &str) -> Result<bool, ConversationError> {
        let conn = self.pool.get()?;
        let mut stmt = conn.prepare(&format!(
            "SELECT EXISTS(SELECT 1 FROM {} WHERE id = ?1)",
            Self::TABLE_NAME
        ))?;
        let exists = stmt.query_row(params![id], |row| row.get(0))?;
        Ok(exists)
    }

    pub fn get(&mut self, id: &str) -> Result<Option<Conversation>, ConversationError> {
        let conn = self.pool.get()?;
        let mut stmt =
            conn.prepare(&format!("SELECT * FROM {} WHERE id = ?1", Self::TABLE_NAME))?;

        let result = stmt.query_row(params![id], |row| {
            Ok(Conversation {
                id: row.get(0)?,
                name: row.get(1)?,
                description: row.get(2)?,
                entry_message_id: row.get(3)?,
                default_pal_id: row.get(4).ok().flatten(),
            })
        });

        match result {
            Ok(conv) => Ok(Some(conv)),
            Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
            Err(e) => Err(e.into()),
        }
    }

    pub fn get_by_entry_id(&mut self, id: &str) -> Result<Option<Conversation>, ConversationError> {
        let conn = self.pool.get()?;
        let mut stmt = conn.prepare(&format!(
            "SELECT * FROM {} WHERE entry_message_id = ?1",
            Self::TABLE_NAME
        ))?;

        let result = stmt.query_row(params![id], |row| {
            Ok(Conversation {
                id: row.get(0)?,
                name: row.get(1)?,
                description: row.get(2)?,
                entry_message_id: row.get(3)?,
                default_pal_id: row.get(4).ok().flatten(),
            })
        });

        match result {
            Ok(conv) => Ok(Some(conv)),
            Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
            Err(e) => Err(e.into()),
        }
    }

    pub fn update_name(&mut self, id: &str, name: &str) -> Result<(), ConversationError> {
        let conn = self.pool.get()?;
        conn.execute(
            &format!("UPDATE {} SET name = ?2 WHERE id = ?1", Self::TABLE_NAME),
            params![id, name],
        )?;
        Ok(())
    }

    pub fn update_description(
        &mut self,
        id: &str,
        description: &str,
    ) -> Result<(), ConversationError> {
        let conn = self.pool.get()?;
        conn.execute(
            &format!("UPDATE {} SET description = ?2 WHERE id = ?1", Self::TABLE_NAME),
            params![id, description],
        )?;
        Ok(())
    }

    pub fn update_entry_message_id(
        &mut self,
        id: &str,
        entry_message_id: Option<&str>,
    ) -> Result<(), ConversationError> {
        let conn = self.pool.get()?;
        conn.execute(
            &format!("UPDATE {} SET entry_message_id = ?2 WHERE id = ?1", Self::TABLE_NAME),
            params![id, entry_message_id],
        )?;
        Ok(())
    }

    pub fn update_default_pal_id(
        &mut self,
        id: &str,
        default_pal_id: Option<&str>,
    ) -> Result<(), ConversationError> {
        let conn = self.pool.get()?;
        conn.execute(
            &format!("UPDATE {} SET default_pal_id = ?2 WHERE id = ?1", Self::TABLE_NAME),
            params![id, default_pal_id],
        )?;
        Ok(())
    }

    pub fn delete(&mut self, id: &str) -> Result<(), ConversationError> {
        let conn = self.pool.get()?;
        conn.execute(
            &format!("DELETE FROM {} WHERE id = ?1", Self::TABLE_NAME),
            params![id],
        )?;
        Ok(())
    }

    pub fn list(&mut self) -> Result<Vec<Conversation>, ConversationError> {
        let conn = self.pool.get()?;
        let mut stmt =
            conn.prepare(&format!("SELECT * FROM {} ORDER BY name ASC", Self::TABLE_NAME))?;

        let conversations = stmt
            .query_map(params![], |row| {
                Ok(Conversation {
                    id: row.get(0)?,
                    name: row.get(1)?,
                    description: row.get(2)?,
                    entry_message_id: row.get(3)?,
                    default_pal_id: row.get(4).ok().flatten(),
                })
            })?
            .collect::<Result<Vec<_>, rusqlite::Error>>()?;

        Ok(conversations)
    }

    /// Returns the persisted branch-selection indices for a conversation, or
    /// `None` if none have been saved (e.g. a fresh conversation). Invalid /
    /// unparseable payloads are treated as absent so the caller can fall back
    /// to the default path.
    pub fn get_thread_decisions(
        &mut self,
        id: &str,
    ) -> Result<Option<Vec<i64>>, ConversationError> {
        let conn = self.pool.get()?;
        let mut stmt = conn.prepare(&format!(
            "SELECT thread_decisions FROM {} WHERE id = ?1",
            Self::TABLE_NAME
        ))?;
        let raw: Option<String> = stmt.query_row(params![id], |row| row.get(0))?;
        match raw {
            None => Ok(None),
            Some(s) => match serde_json::from_str::<Vec<i64>>(&s) {
                Ok(v) if v.iter().all(|n| *n >= 0) => Ok(Some(v)),
                _ => Ok(None),
            },
        }
    }

    /// Persists the branch-selection indices for a conversation. `None`
    /// clears the stored value.
    pub fn update_thread_decisions(
        &mut self,
        id: &str,
        decisions: Option<&[i64]>,
    ) -> Result<(), ConversationError> {
        let conn = self.pool.get()?;
        let payload: rusqlite::types::Value = match decisions {
            Some(d) => serde_json::to_string(d)
                .map_err(|e| {
                    ConversationError::Database(rusqlite::Error::ToSqlConversionFailure(Box::new(
                        e,
                    )))
                })?
                .into(),
            None => rusqlite::types::Null.into(),
        };
        conn.execute(
            &format!("UPDATE {} SET thread_decisions = ?2 WHERE id = ?1", Self::TABLE_NAME),
            params![id, payload],
        )?;
        Ok(())
    }
}
