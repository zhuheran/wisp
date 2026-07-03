use rusqlite::params;
use crate::pool::DbPool;
use crate::types::{Conversation, ConversationError};

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

        Ok(Self { pool })
    }

	pub fn create(&mut self, id: &str, name: &str, description: Option<&str>, entry_message_id: Option<&str>, default_pal_id: Option<&str>) -> Result<(), ConversationError> {
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
		let mut stmt = conn.prepare(&format!(
			"SELECT * FROM {} WHERE id = ?1",
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
			&format!(
				"UPDATE {} SET name = ?2 WHERE id = ?1",
				Self::TABLE_NAME
			),
			params![id, name],
		)?;
		Ok(())
	}

	pub fn update_description(&mut self, id: &str, description: &str) -> Result<(), ConversationError> {
		let conn = self.pool.get()?;
		conn.execute(
			&format!(
				"UPDATE {} SET description = ?2 WHERE id = ?1",
				Self::TABLE_NAME
			),
			params![id, description],
		)?;
		Ok(())
	}

	pub fn update_entry_message_id(&mut self, id: &str, entry_message_id: Option<&str>) -> Result<(), ConversationError> {
		let conn = self.pool.get()?;
		conn.execute(
			&format!(
				"UPDATE {} SET entry_message_id = ?2 WHERE id = ?1",
				Self::TABLE_NAME
			),
			params![id, entry_message_id],
		)?;
		Ok(())
	}

	pub fn update_default_pal_id(&mut self, id: &str, default_pal_id: Option<&str>) -> Result<(), ConversationError> {
		let conn = self.pool.get()?;
		conn.execute(
			&format!(
				"UPDATE {} SET default_pal_id = ?2 WHERE id = ?1",
				Self::TABLE_NAME
			),
			params![id, default_pal_id],
		)?;
		Ok(())
	}

	pub fn delete(&mut self, id: &str) -> Result<(), ConversationError> {
		let conn = self.pool.get()?;
		conn.execute(
			&format!(
				"DELETE FROM {} WHERE id = ?1",
				Self::TABLE_NAME
			),
			params![id],
		)?;
		Ok(())
	}

	pub fn list(&mut self) -> Result<Vec<Conversation>, ConversationError> {
		let conn = self.pool.get()?;
		let mut stmt = conn.prepare(&format!(
			"SELECT * FROM {} ORDER BY name ASC",
			Self::TABLE_NAME
		))?;

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

}
