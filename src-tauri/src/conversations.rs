use rusqlite::{params, Connection};
use serde_json::Value;
use std::{fs, path::PathBuf};

const DATABASE_DIRECTORY: &str = "AI-OS";
const DATABASE_FILE: &str = "conversations.sqlite3";

fn database_path() -> Result<PathBuf, String> {
    let directory = dirs::data_dir()
        .ok_or_else(|| "AI-OS data directory is unavailable".to_owned())?
        .join(DATABASE_DIRECTORY);
    fs::create_dir_all(&directory)
        .map_err(|_| "AI-OS could not create its conversation storage".to_owned())?;
    Ok(directory.join(DATABASE_FILE))
}

fn open_database() -> Result<Connection, String> {
    let connection = Connection::open(database_path()?)
        .map_err(|_| "AI-OS could not open its conversation database".to_owned())?;
    connection
        .execute_batch(
            "
            PRAGMA journal_mode = WAL;
            PRAGMA foreign_keys = ON;
            CREATE TABLE IF NOT EXISTS conversations (
                id TEXT PRIMARY KEY NOT NULL,
                title TEXT NOT NULL,
                payload TEXT NOT NULL,
                created_at TEXT NOT NULL,
                updated_at TEXT NOT NULL
            );
            CREATE INDEX IF NOT EXISTS conversations_updated_at
                ON conversations(updated_at DESC);
            ",
        )
        .map_err(|_| "AI-OS could not initialize its conversation database".to_owned())?;
    Ok(connection)
}

fn conversation_field<'a>(conversation: &'a Value, field: &str) -> Result<&'a str, String> {
    conversation
        .get(field)
        .and_then(Value::as_str)
        .filter(|value| !value.trim().is_empty())
        .ok_or_else(|| format!("Conversation {field} is invalid"))
}

fn validate_conversation(conversation: &Value) -> Result<(), String> {
    let id = conversation_field(conversation, "id")?;
    if id.len() > 128
        || !id
            .chars()
            .all(|character| character.is_ascii_alphanumeric() || matches!(character, '-' | '_'))
    {
        return Err("Conversation identifier is invalid".to_owned());
    }
    if conversation_field(conversation, "title")?.len() > 200 {
        return Err("Conversation title is too long".to_owned());
    }
    conversation_field(conversation, "createdAt")?;
    conversation_field(conversation, "updatedAt")?;
    if !conversation
        .get("messages")
        .is_some_and(|messages| messages.is_array())
    {
        return Err("Conversation messages are invalid".to_owned());
    }
    Ok(())
}

fn save_conversation(connection: &Connection, conversation: &Value) -> Result<(), String> {
    validate_conversation(conversation)?;
    let payload = serde_json::to_string(conversation)
        .map_err(|_| "Conversation could not be serialized".to_owned())?;
    if payload.len() > 16 * 1024 * 1024 {
        return Err("Conversation is too large to save".to_owned());
    }
    connection
        .execute(
            "
            INSERT INTO conversations (id, title, payload, created_at, updated_at)
            VALUES (?1, ?2, ?3, ?4, ?5)
            ON CONFLICT(id) DO UPDATE SET
                title = excluded.title,
                payload = excluded.payload,
                updated_at = excluded.updated_at
            ",
            params![
                conversation_field(conversation, "id")?,
                conversation_field(conversation, "title")?,
                payload,
                conversation_field(conversation, "createdAt")?,
                conversation_field(conversation, "updatedAt")?,
            ],
        )
        .map_err(|_| "AI-OS could not save the conversation".to_owned())?;
    Ok(())
}

#[tauri::command]
pub(crate) fn list_native_conversations() -> Result<Vec<Value>, String> {
    let connection = open_database()?;
    let mut statement = connection
        .prepare("SELECT payload FROM conversations ORDER BY updated_at DESC")
        .map_err(|_| "AI-OS could not read conversations".to_owned())?;
    let rows = statement
        .query_map([], |row| row.get::<_, String>(0))
        .map_err(|_| "AI-OS could not read conversations".to_owned())?;
    let mut conversations = Vec::new();
    for row in rows {
        let payload = row.map_err(|_| "AI-OS could not read a conversation".to_owned())?;
        let conversation = serde_json::from_str(&payload)
            .map_err(|_| "A saved conversation is unreadable".to_owned())?;
        conversations.push(conversation);
    }
    Ok(conversations)
}

#[tauri::command]
pub(crate) fn save_native_conversation(conversation: Value) -> Result<(), String> {
    save_conversation(&open_database()?, &conversation)
}

#[tauri::command]
pub(crate) fn delete_native_conversation(id: String) -> Result<(), String> {
    if id.len() > 128
        || !id
            .chars()
            .all(|character| character.is_ascii_alphanumeric() || matches!(character, '-' | '_'))
    {
        return Err("Conversation identifier is invalid".to_owned());
    }
    open_database()?
        .execute("DELETE FROM conversations WHERE id = ?1", params![id])
        .map_err(|_| "AI-OS could not delete the conversation".to_owned())?;
    Ok(())
}

#[tauri::command]
pub(crate) fn import_native_conversations(conversations: Vec<Value>) -> Result<usize, String> {
    let mut connection = open_database()?;
    let count: i64 = connection
        .query_row("SELECT COUNT(*) FROM conversations", [], |row| row.get(0))
        .map_err(|_| "AI-OS could not inspect conversation storage".to_owned())?;
    if count > 0 {
        return Ok(0);
    }
    let transaction = connection
        .transaction()
        .map_err(|_| "AI-OS could not start conversation migration".to_owned())?;
    for conversation in &conversations {
        save_conversation(&transaction, conversation)?;
    }
    transaction
        .commit()
        .map_err(|_| "AI-OS could not finish conversation migration".to_owned())?;
    Ok(conversations.len())
}
