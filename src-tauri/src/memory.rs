use rusqlite::{params, Connection};
use serde_json::Value;
use std::path::PathBuf;

const DATABASE_FILE: &str = "memory.sqlite3";

fn database_path() -> Result<PathBuf, String> {
    let dir =
        dirs::data_local_dir().ok_or_else(|| "AI-OS could not locate memory storage".to_owned())?;

    std::fs::create_dir_all(&dir)
        .map_err(|_| "AI-OS could not create memory storage".to_owned())?;

    Ok(dir.join(DATABASE_FILE))
}

fn open_database() -> Result<Connection, String> {
    let connection = Connection::open(database_path()?)
        .map_err(|_| "AI-OS could not open memory database".to_owned())?;

    connection
        .execute_batch(
            "
            CREATE TABLE IF NOT EXISTS memory_entries (
                id TEXT PRIMARY KEY,
                memory_type TEXT NOT NULL,
                content TEXT NOT NULL,
                metadata TEXT,
                created_at TEXT NOT NULL,
                updated_at TEXT NOT NULL
            );

            CREATE INDEX IF NOT EXISTS memory_entries_updated_at
            ON memory_entries(updated_at DESC);
            ",
        )
        .map_err(|_| "AI-OS could not initialize memory database".to_owned())?;

    Ok(connection)
}

#[tauri::command]
pub(crate) fn save_memory(entry: Value) -> Result<(), String> {
    let connection = open_database()?;

    let id = entry
        .get("id")
        .and_then(Value::as_str)
        .ok_or_else(|| "memory id missing".to_owned())?;

    let memory_type = entry
        .get("type")
        .and_then(Value::as_str)
        .unwrap_or("general");

    let content = entry
        .get("content")
        .and_then(Value::as_str)
        .ok_or_else(|| "memory content missing".to_owned())?;

    let metadata = entry.get("metadata").map(|value| value.to_string());

    let now = chrono::Utc::now().to_rfc3339();

    connection
        .execute(
            "
            INSERT INTO memory_entries
            (id, memory_type, content, metadata, created_at, updated_at)
            VALUES (?1, ?2, ?3, ?4, ?5, ?6)
            ON CONFLICT(id) DO UPDATE SET
                content = excluded.content,
                metadata = excluded.metadata,
                updated_at = excluded.updated_at
            ",
            params![id, memory_type, content, metadata, now, now],
        )
        .map_err(|_| "AI-OS could not save memory".to_owned())?;

    Ok(())
}

#[tauri::command]
pub(crate) fn list_memory() -> Result<Vec<Value>, String> {
    let connection = open_database()?;

    let mut statement = connection
        .prepare(
            "
            SELECT id, memory_type, content, metadata, created_at, updated_at
            FROM memory_entries
            ORDER BY updated_at DESC
            ",
        )
        .map_err(|_| "AI-OS could not read memory".to_owned())?;

    let rows = statement
        .query_map([], |row| {
            Ok(serde_json::json!({
                "id": row.get::<_, String>(0)?,
                "type": row.get::<_, String>(1)?,
                "content": row.get::<_, String>(2)?,
                "metadata": row.get::<_, Option<String>>(3)?,
                "createdAt": row.get::<_, String>(4)?,
                "updatedAt": row.get::<_, String>(5)?
            }))
        })
        .map_err(|_| "AI-OS could not read memory".to_owned())?;

    let mut result = Vec::new();

    for row in rows {
        result.push(row.map_err(|_| "AI-OS could not read memory item".to_owned())?);
    }

    Ok(result)
}

#[tauri::command]
pub(crate) fn delete_memory(id: String) -> Result<(), String> {
    let connection = open_database()?;

    connection
        .execute("DELETE FROM memory_entries WHERE id = ?1", params![id])
        .map_err(|_| "AI-OS could not delete memory".to_owned())?;

    Ok(())
}
