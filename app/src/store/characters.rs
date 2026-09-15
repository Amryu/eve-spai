//! Authenticated characters, their token expiry and which account each belongs to.

use super::*;

impl Store {
    pub fn list_characters(&self) -> Vec<CharacterRow> {
        let mut out = Vec::new();
        if let Ok(mut stmt) = self.conn.prepare(
            "SELECT id, name, COALESCE(expires_at, 0), COALESCE(scopes, '')
             FROM characters ORDER BY name",
        ) {
            if let Ok(rows) = stmt.query_map([], |row| {
                Ok(CharacterRow {
                    id: row.get(0)?,
                    name: row.get(1)?,
                    expires_at: row.get(2)?,
                    scopes: row.get(3)?,
                })
            }) {
                out.extend(rows.flatten());
            }
        }
        out
    }

    pub fn character_by_name(&self, name: &str) -> Option<CharacterRow> {
        self.list_characters().into_iter().find(|c| c.name == name)
    }

    pub fn update_token_expiry(&self, id: i64, expires_at: i64) -> Result<()> {
        self.conn.execute(
            "UPDATE characters SET expires_at = ?1 WHERE id = ?2",
            params![expires_at, id],
        )?;
        Ok(())
    }

    pub fn token_expiry(&self, id: i64) -> Option<i64> {
        self.conn
            .query_row("SELECT expires_at FROM characters WHERE id = ?1", params![id], |r| {
                r.get::<_, Option<i64>>(0)
            })
            .ok()
            .flatten()
    }

    /// Records an association unless a more trustworthy one is already stored, so a guess can
    /// never quietly replace the user's manual assignment or an exact read off a live client.
    pub fn set_char_account(&self, character_id: i64, account_id: i64, source: AssocSource) {
        let existing: Option<(i64, String)> = self
            .conn
            .query_row(
                "SELECT account_id, source FROM char_accounts WHERE character_id = ?1",
                params![character_id],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .ok();
        if let Some((have_account, have_source)) = existing {
            let have = AssocSource::from_str(&have_source);
            if have.rank() > source.rank() || (have.rank() == source.rank() && have_account == account_id)
            {
                return;
            }
        }
        let _ = self.conn.execute(
            "INSERT INTO char_accounts (character_id, account_id, source, seen_at)
             VALUES (?1, ?2, ?3, ?4)
             ON CONFLICT(character_id) DO UPDATE SET
                 account_id = excluded.account_id,
                 source     = excluded.source,
                 seen_at    = excluded.seen_at",
            params![character_id, account_id, source.as_str(), chrono::Utc::now().timestamp()],
        );
    }

    pub fn char_accounts(&self) -> std::collections::HashMap<i64, (i64, AssocSource)> {
        let mut out = std::collections::HashMap::new();
        if let Ok(mut stmt) =
            self.conn.prepare("SELECT character_id, account_id, source FROM char_accounts")
        {
            if let Ok(rows) = stmt.query_map([], |r| {
                Ok((
                    r.get::<_, i64>(0)?,
                    r.get::<_, i64>(1)?,
                    r.get::<_, String>(2)?,
                ))
            }) {
                for (c, a, s) in rows.flatten() {
                    out.insert(c, (a, AssocSource::from_str(&s)));
                }
            }
        }
        out
    }

    pub fn clear_char_account(&self, character_id: i64) {
        let _ = self
            .conn
            .execute("DELETE FROM char_accounts WHERE character_id = ?1", params![character_id]);
    }

    pub fn set_char_name(&self, character_id: i64, name: &str) {
        let _ = self.conn.execute(
            "INSERT INTO char_names (character_id, name, fetched_at) VALUES (?1, ?2, ?3)
             ON CONFLICT(character_id) DO UPDATE SET name = excluded.name,
                                                     fetched_at = excluded.fetched_at",
            params![character_id, name, chrono::Utc::now().timestamp()],
        );
    }

    pub fn char_names(&self) -> std::collections::HashMap<i64, String> {
        let mut out = std::collections::HashMap::new();
        if let Ok(mut stmt) = self.conn.prepare("SELECT character_id, name FROM char_names") {
            if let Ok(rows) =
                stmt.query_map([], |r| Ok((r.get::<_, i64>(0)?, r.get::<_, String>(1)?)))
            {
                out.extend(rows.flatten());
            }
        }
        out
    }

    pub fn remove_character(&self, id: i64) -> Result<()> {
        let _ = crate::tokens::delete(id);
        self.kv_delete(&format!("access:{id}"));
        self.conn
            .execute("DELETE FROM characters WHERE id = ?1", params![id])?;
        Ok(())
    }
}
