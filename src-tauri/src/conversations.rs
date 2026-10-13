use rusqlite::{params, Connection};
use serde_json::Value;
use std::{fs, path::PathBuf, sync::Mutex};

pub(crate) static PROJECT_CONVERSATION_GATE: Mutex<()> = Mutex::new(());


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
    let _guard = PROJECT_CONVERSATION_GATE
        .lock()
        .map_err(|_| "Conversation synchronization is unavailable".to_owned())?;

    let mut conversation = conversation.clone();
    validate_conversation(&conversation)?;

    if let Some(project_id) = conversation
        .get("projectId")
        .and_then(Value::as_str)
        .map(str::to_owned)
    {
        if !crate::knowledge::knowledge_project_exists(&project_id)? {
            if let Some(object) = conversation.as_object_mut() {
                object.remove("projectId");
            }
        }
    }

    let payload = serde_json::to_string(&conversation)
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
                conversation_field(&conversation, "id")?,
                conversation_field(&conversation, "title")?,
                payload,
                conversation_field(&conversation, "createdAt")?,
                conversation_field(&conversation, "updatedAt")?,
            ],
        )
        .map_err(|_| "AI-OS could not save the conversation".to_owned())?;
    Ok(())
}


pub(crate) fn detach_project_conversations(
    project_id: &str,
) -> Result<(), String> {
    let mut connection = open_database()?;
    let transaction = connection
        .transaction()
        .map_err(|_| "Could not start conversation update".to_owned())?;

    let affected: Vec<(String, String)> = {
        let mut statement = transaction
            .prepare("SELECT id, payload FROM conversations")
            .map_err(|_| "Could not read project conversations".to_owned())?;

        let rows = statement
            .query_map([], |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                ))
            })
            .map_err(|_| "Could not read project conversations".to_owned())?;

        let mut affected = Vec::new();

        for row in rows {
            let (id, payload) = row
                .map_err(|_| "Could not read a conversation".to_owned())?;

            let mut value: Value = serde_json::from_str(&payload)
                .map_err(|_| "A conversation is unreadable".to_owned())?;

            if value.get("projectId").and_then(Value::as_str)
                != Some(project_id)
            {
                continue;
            }

            let object = value.as_object_mut()
                .ok_or_else(|| "A conversation is invalid".to_owned())?;

            object.remove("projectId");

            let updated = serde_json::to_string(&value)
                .map_err(|_| "Could not serialize conversation".to_owned())?;

            affected.push((id, updated));
        }

        affected
    };

    for (id, payload) in affected {
        transaction
            .execute(
                "UPDATE conversations SET payload = ?1 WHERE id = ?2",
                params![payload, id],
            )
            .map_err(|_| "Could not update project conversations".to_owned())?;
    }

    transaction
        .commit()
        .map_err(|_| "Could not save conversation changes".to_owned())?;

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
