use chrono::Utc;
use rusqlite::{params, Connection};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::{collections::HashSet, fs};

const MAX_SOURCE_BYTES: u64 = 5 * 1024 * 1024;
const MAX_SOURCE_CHARS: usize = 256 * 1024;
const MAX_SEARCH_RESULTS: usize = 12;

fn valid_id(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 128
        && value
            .chars()
            .all(|character| character.is_ascii_alphanumeric() || matches!(character, '-' | '_'))
}

fn open_database() -> Result<Connection, String> {
    let connection = crate::memory::open_database()?;
    initialize_database(&connection)?;
    Ok(connection)
}

fn initialize_database(connection: &Connection) -> Result<(), String> {
    connection
        .execute_batch(
            "
            PRAGMA foreign_keys = ON;
            CREATE TABLE IF NOT EXISTS knowledge_projects (
                id TEXT PRIMARY KEY NOT NULL,
                name TEXT NOT NULL,
                created_at TEXT NOT NULL,
                updated_at TEXT NOT NULL
            );
            CREATE TABLE IF NOT EXISTS knowledge_sources (
                id TEXT PRIMARY KEY NOT NULL,
                project_id TEXT NOT NULL,
                path TEXT NOT NULL,
                name TEXT NOT NULL,
                source_kind TEXT NOT NULL,
                content TEXT NOT NULL,
                created_at TEXT NOT NULL,
                updated_at TEXT NOT NULL,
                UNIQUE(project_id, path),
                FOREIGN KEY(project_id) REFERENCES knowledge_projects(id) ON DELETE CASCADE
            );
            CREATE INDEX IF NOT EXISTS knowledge_sources_project
                ON knowledge_sources(project_id, updated_at DESC);
            ",
        )
        .map_err(|_| "AI-OS could not initialize project knowledge storage".to_owned())?;
    Ok(())
}

#[tauri::command]
pub(crate) fn save_knowledge_project(project: Value) -> Result<(), String> {
    let id = project
        .get("id")
        .and_then(Value::as_str)
        .filter(|value| valid_id(value))
        .ok_or_else(|| "Knowledge project identifier is invalid".to_owned())?;
    let name = project
        .get("name")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty() && value.len() <= 120)
        .ok_or_else(|| "Knowledge project name is invalid".to_owned())?;
    let now = Utc::now().to_rfc3339();
    open_database()?
        .execute(
            "INSERT INTO knowledge_projects (id, name, created_at, updated_at)
             VALUES (?1, ?2, ?3, ?3)
             ON CONFLICT(id) DO UPDATE SET name = excluded.name, updated_at = excluded.updated_at",
            params![id, name, now],
        )
        .map_err(|_| "AI-OS could not save the knowledge project".to_owned())?;
    Ok(())
}

#[tauri::command]
pub(crate) fn list_knowledge_projects() -> Result<Vec<Value>, String> {
    let connection = open_database()?;
    let mut statement = connection
        .prepare("SELECT id, name, created_at, updated_at FROM knowledge_projects ORDER BY updated_at DESC")
        .map_err(|_| "AI-OS could not read knowledge projects".to_owned())?;
    let rows = statement
        .query_map([], |row| {
            Ok(json!({
                "id": row.get::<_, String>(0)?,
                "name": row.get::<_, String>(1)?,
                "createdAt": row.get::<_, String>(2)?,
                "updatedAt": row.get::<_, String>(3)?,
            }))
        })
        .map_err(|_| "AI-OS could not read knowledge projects".to_owned())?;
    rows.map(|row| row.map_err(|_| "A knowledge project is unreadable".to_owned()))
        .collect()
}

fn extracted_text(path: &str, extension: &str) -> Result<(String, &'static str), String> {
    let input = json!({"path": path});
    let (result, source_kind) = match extension {
        "pdf" => crate::document::pdf::read_pdf_document(&input)
            .map(|result| (result, "pdf"))
            .map_err(|error| error.message)?,
        "docx" => crate::document::structured::read_structured_document(&input)
            .map(|result| (result, "word"))
            .map_err(|error| error.message)?,
        "doc" => crate::document::word::read_word_document(&input)
            .map(|result| (result, "word"))
            .map_err(|error| error.message)?,
        "pptx" => crate::document::structured::read_structured_presentation(&input)
            .map(|result| (result, "presentation"))
            .map_err(|error| error.message)?,
        "xlsx" => crate::document::structured::read_structured_spreadsheet(&input)
            .map(|result| (result, "spreadsheet"))
            .map_err(|error| error.message)?,
        _ => {
            let metadata = fs::metadata(path)
                .map_err(|_| "The selected knowledge source is unavailable".to_owned())?;
            if metadata.len() > MAX_SOURCE_BYTES {
                return Err("The selected text or code file is larger than 5 MB".to_owned());
            }
            let text = fs::read_to_string(path)
                .map_err(|_| "The selected source is not readable text".to_owned())?;
            return Ok((text.chars().take(MAX_SOURCE_CHARS).collect(), "text"));
        }
    };
    let text_value = if source_kind == "presentation" {
        result
            .pointer("/operationResult/slides")
            .and_then(Value::as_array)
            .map(|slides| {
                slides
                    .iter()
                    .filter_map(|slide| slide.get("text").and_then(Value::as_str))
                    .collect::<Vec<_>>()
                    .join("\n")
            })
            .unwrap_or_default()
    } else if source_kind == "spreadsheet" {
        result
            .get("content")
            .map(Value::to_string)
            .or_else(|| result.get("sheets").map(Value::to_string))
            .unwrap_or_default()
    } else {
        result
            .pointer("/operationResult/text")
            .and_then(Value::as_str)
            .unwrap_or("")
            .to_owned()
    };
    let text = text_value
        .chars()
        .take(MAX_SOURCE_CHARS)
        .collect::<String>();
    if text.trim().is_empty() {
        return Err("The selected source contains no retrievable text".to_owned());
    }
    Ok((text, source_kind))
}

#[tauri::command]
pub(crate) fn index_knowledge_source(project_id: String, path: String) -> Result<Value, String> {
    if !valid_id(&project_id) {
        return Err("Knowledge project identifier is invalid".to_owned());
    }
    let canonical = fs::canonicalize(path.trim())
        .map_err(|_| "The selected knowledge source does not exist".to_owned())?;
    if !canonical.is_file() {
        return Err("The selected knowledge source is not a file".to_owned());
    }
    let extension = canonical
        .extension()
        .and_then(|value| value.to_str())
        .map(str::to_ascii_lowercase)
        .unwrap_or_default();
    let supported = [
        "pdf", "doc", "docx", "pptx", "xlsx", "txt", "md", "json", "csv", "tsv", "js", "jsx", "ts",
        "tsx", "py", "rs", "go", "java", "kt", "swift", "c", "h", "cpp", "hpp", "html", "css",
        "yaml", "yml", "toml", "sql", "sh",
    ];
    if !supported.contains(&extension.as_str()) {
        return Err("This file type is not supported for project knowledge".to_owned());
    }
    let canonical_string = canonical.to_string_lossy().to_string();
    let name = canonical
        .file_name()
        .and_then(|value| value.to_str())
        .unwrap_or("Knowledge source")
        .to_owned();
    let (content, source_kind) = extracted_text(&canonical_string, &extension)?;
    let mut hasher = Sha256::new();
    hasher.update(project_id.as_bytes());
    hasher.update(b"\0");
    hasher.update(canonical_string.as_bytes());
    let id = format!("source-{:x}", hasher.finalize());
    let now = Utc::now().to_rfc3339();
    let connection = open_database()?;
    let exists: i64 = connection
        .query_row(
            "SELECT COUNT(*) FROM knowledge_projects WHERE id = ?1",
            params![project_id],
            |row| row.get(0),
        )
        .map_err(|_| "AI-OS could not inspect the knowledge project".to_owned())?;
    if exists == 0 {
        return Err("The selected knowledge project no longer exists".to_owned());
    }
    connection
        .execute(
            "INSERT INTO knowledge_sources
             (id, project_id, path, name, source_kind, content, created_at, updated_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?7)
             ON CONFLICT(project_id, path) DO UPDATE SET
               name = excluded.name, source_kind = excluded.source_kind,
               content = excluded.content, updated_at = excluded.updated_at",
            params![
                id,
                project_id,
                canonical_string,
                name,
                source_kind,
                content,
                now
            ],
        )
        .map_err(|_| "AI-OS could not index the knowledge source".to_owned())?;
    Ok(json!({
        "id": id,
        "projectId": project_id,
        "name": name,
        "path": canonical_string,
        "kind": source_kind,
        "updatedAt": now,
    }))
}

#[tauri::command]
pub(crate) fn list_knowledge_sources(project_id: String) -> Result<Vec<Value>, String> {
    if !valid_id(&project_id) {
        return Err("Knowledge project identifier is invalid".to_owned());
    }
    let connection = open_database()?;
    let mut statement = connection
        .prepare(
            "SELECT id, project_id, path, name, source_kind, updated_at
             FROM knowledge_sources WHERE project_id = ?1 ORDER BY updated_at DESC",
        )
        .map_err(|_| "AI-OS could not read knowledge sources".to_owned())?;
    let rows = statement
        .query_map(params![project_id], |row| {
            Ok(json!({
                "id": row.get::<_, String>(0)?,
                "projectId": row.get::<_, String>(1)?,
                "path": row.get::<_, String>(2)?,
                "name": row.get::<_, String>(3)?,
                "kind": row.get::<_, String>(4)?,
                "updatedAt": row.get::<_, String>(5)?,
            }))
        })
        .map_err(|_| "AI-OS could not read knowledge sources".to_owned())?;
    rows.map(|row| row.map_err(|_| "A knowledge source is unreadable".to_owned()))
        .collect()
}

#[tauri::command]
pub(crate) fn remove_knowledge_source(project_id: String, id: String) -> Result<(), String> {
    if !valid_id(&project_id) || !valid_id(&id) {
        return Err("Knowledge source identity is invalid".to_owned());
    }
    open_database()?
        .execute(
            "DELETE FROM knowledge_sources WHERE project_id = ?1 AND id = ?2",
            params![project_id, id],
        )
        .map_err(|_| "AI-OS could not remove the knowledge source".to_owned())?;
    Ok(())
}

fn search_terms(query: &str) -> Vec<String> {
    let normalized = query.to_lowercase();
    let mut terms = Vec::new();
    let mut current = String::new();
    for character in normalized.chars() {
        if character.is_alphanumeric() || matches!(character, '_' | '-' | '.') {
            current.push(character);
            if ('\u{3400}'..='\u{9fff}').contains(&character) {
                terms.push(character.to_string());
            }
        } else if current.chars().count() > 1 {
            terms.push(std::mem::take(&mut current));
        } else {
            current.clear();
        }
    }
    if current.chars().count() > 1 {
        terms.push(current);
    }
    let mut seen = HashSet::new();
    terms.retain(|term| seen.insert(term.clone()));
    terms.truncate(24);
    terms
}

fn excerpts(content: &str) -> impl Iterator<Item = String> + '_ {
    content
        .split(|character| matches!(character, '\n' | '\r'))
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .map(|line| line.chars().take(700).collect())
}

fn search_database(
    connection: &Connection,
    project_id: &str,
    terms: &[String],
    limit: usize,
) -> Result<Vec<Value>, String> {
    let mut statement = connection
        .prepare("SELECT id, name, path, source_kind, content FROM knowledge_sources WHERE project_id = ?1")
        .map_err(|_| "AI-OS could not search project knowledge".to_owned())?;
    let rows = statement
        .query_map(params![project_id], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
                row.get::<_, String>(3)?,
                row.get::<_, String>(4)?,
            ))
        })
        .map_err(|_| "AI-OS could not search project knowledge".to_owned())?;
    let mut matches = Vec::new();
    for row in rows {
        let (source_id, name, path, kind, content) =
            row.map_err(|_| "A knowledge source is unreadable".to_owned())?;
        for (index, excerpt) in excerpts(&content).enumerate() {
            let haystack = excerpt.to_lowercase();
            let matched = terms
                .iter()
                .filter(|term| haystack.contains(term.as_str()))
                .count();
            if matched == 0 {
                continue;
            }
            matches.push((
                matched,
                json!({
                    "sourceId": source_id,
                    "sourceName": name,
                    "sourcePath": path,
                    "kind": kind,
                    "locator": format!("line:{}", index + 1),
                    "excerpt": excerpt,
                    "score": matched,
                }),
            ));
        }
    }
    matches.sort_by(|left, right| right.0.cmp(&left.0));
    Ok(matches
        .into_iter()
        .take(limit.clamp(1, MAX_SEARCH_RESULTS))
        .map(|(_, value)| value)
        .collect())
}

#[tauri::command]
pub(crate) fn search_knowledge(
    project_id: String,
    query: String,
    limit: Option<usize>,
) -> Result<Vec<Value>, String> {
    if !valid_id(&project_id) {
        return Err("Knowledge project identifier is invalid".to_owned());
    }
    let terms = search_terms(&query);
    if terms.is_empty() {
        return Ok(Vec::new());
    }
    search_database(&open_database()?, &project_id, &terms, limit.unwrap_or(6))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn search_terms_cover_words_numbers_and_cjk() {
        let terms = search_terms("Beta4 budget 128000 与项目决定");
        assert!(terms.contains(&"beta4".to_owned()));
        assert!(terms.contains(&"128000".to_owned()));
        assert!(terms.contains(&"项".to_owned()));
    }

    #[test]
    fn source_ids_are_scoped_by_project() {
        let path = "/tmp/example.md";
        let digest = |project: &str| {
            let mut hasher = Sha256::new();
            hasher.update(project.as_bytes());
            hasher.update(b"\0");
            hasher.update(path.as_bytes());
            format!("source-{:x}", hasher.finalize())
        };
        assert_ne!(digest("project-a"), digest("project-b"));
    }

    #[test]
    fn retrieval_is_scoped_and_traceable() {
        let connection = Connection::open_in_memory().unwrap();
        initialize_database(&connection).unwrap();
        for project in ["project-a", "project-b"] {
            connection.execute(
                "INSERT INTO knowledge_projects (id, name, created_at, updated_at) VALUES (?1, ?1, 'now', 'now')",
                params![project],
            ).unwrap();
        }
        connection.execute(
            "INSERT INTO knowledge_sources (id, project_id, path, name, source_kind, content, created_at, updated_at)
             VALUES ('source-a', 'project-a', '/a.md', 'a.md', 'text', 'Approved budget is 128000 AUD', 'now', 'now')",
            [],
        ).unwrap();
        connection.execute(
            "INSERT INTO knowledge_sources (id, project_id, path, name, source_kind, content, created_at, updated_at)
             VALUES ('source-b', 'project-b', '/b.md', 'b.md', 'text', 'Approved budget is 64000 AUD', 'now', 'now')",
            [],
        ).unwrap();
        let terms = search_terms("budget AUD");
        let results = search_database(&connection, "project-a", &terms, 6).unwrap();
        assert_eq!(results.len(), 1);
        assert_eq!(results[0]["sourceId"], "source-a");
        assert_eq!(results[0]["locator"], "line:1");
        assert_eq!(results[0]["excerpt"], "Approved budget is 128000 AUD");
    }

    #[test]
    fn plain_text_extraction_is_bounded_and_exact() {
        let root = tempfile::tempdir().unwrap();
        let path = root.path().join("notes.md");
        fs::write(&path, "Decision: keep the full conversation history.").unwrap();
        let (text, kind) = extracted_text(path.to_str().unwrap(), "md").unwrap();
        assert_eq!(kind, "text");
        assert_eq!(text, "Decision: keep the full conversation history.");
    }
}
