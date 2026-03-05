use dataxlr8_mcp_core::mcp::{empty_schema, error_result, get_i64, get_str, get_str_array, json_result, make_schema};
use dataxlr8_mcp_core::Database;
use rmcp::model::*;
use rmcp::service::{RequestContext, RoleServer};
use rmcp::ServerHandler;
use serde::{Deserialize, Serialize};
use tracing::{error, info, warn};

// ============================================================================
// Constants
// ============================================================================

const DEFAULT_LIMIT: i64 = 50;
const DEFAULT_OFFSET: i64 = 0;
const MAX_LIMIT: i64 = 200;
const MAX_TITLE_LEN: usize = 500;
const MAX_CONTENT_LEN: usize = 100_000;
const MAX_TAG_LEN: usize = 100;
const MAX_TAGS: usize = 50;
const VALID_NOTE_TYPES: &[&str] = &["meeting", "call", "research", "internal"];

// ============================================================================
// Data types
// ============================================================================

#[derive(Debug, Serialize, Deserialize, sqlx::FromRow)]
pub struct Note {
    pub id: String,
    pub note_type: String,
    pub title: String,
    pub content: String,
    pub contact_email: String,
    pub tags: Vec<String>,
    pub created_at: chrono::DateTime<chrono::Utc>,
    pub updated_at: chrono::DateTime<chrono::Utc>,
}

#[derive(Debug, Serialize)]
pub struct NoteStats {
    pub total: i64,
    pub meeting: i64,
    pub call: i64,
    pub research: i64,
    pub internal: i64,
}

// ============================================================================
// Validation helpers
// ============================================================================

/// Clamp limit to [1, MAX_LIMIT], defaulting to the given default.
fn clamp_limit(raw: Option<i64>, default: i64) -> i64 {
    raw.unwrap_or(default).clamp(1, MAX_LIMIT)
}

/// Clamp offset to [0, i64::MAX], defaulting to 0.
fn clamp_offset(raw: Option<i64>) -> i64 {
    raw.unwrap_or(DEFAULT_OFFSET).max(0)
}

/// Basic email format check: contains exactly one '@' with non-empty local and domain parts.
fn is_valid_email(email: &str) -> bool {
    let parts: Vec<&str> = email.split('@').collect();
    parts.len() == 2 && !parts[0].is_empty() && parts[1].contains('.')
}

/// Validate tags: each tag must be non-empty, within length, and total count within limit.
fn validate_tags(tags: &[String]) -> Result<Vec<String>, String> {
    if tags.len() > MAX_TAGS {
        return Err(format!("Too many tags: {} (max {})", tags.len(), MAX_TAGS));
    }
    let trimmed: Vec<String> = tags.iter().map(|t| t.trim().to_string()).collect();
    for tag in &trimmed {
        if tag.is_empty() {
            return Err("Tags must not be empty strings".to_string());
        }
        if tag.len() > MAX_TAG_LEN {
            return Err(format!(
                "Tag '{}...' exceeds max length of {} chars",
                &tag[..20.min(tag.len())],
                MAX_TAG_LEN
            ));
        }
    }
    Ok(trimmed)
}

// ============================================================================
// Tool definitions
// ============================================================================

fn build_tools() -> Vec<Tool> {
    vec![
        Tool {
            name: "create_note".into(),
            title: None,
            description: Some("Create a new note linked to a contact".into()),
            input_schema: make_schema(
                serde_json::json!({
                    "note_type": { "type": "string", "enum": ["meeting", "call", "research", "internal"], "description": "Type of note" },
                    "title": { "type": "string", "description": "Note title" },
                    "content": { "type": "string", "description": "Note body text" },
                    "contact_email": { "type": "string", "description": "Email of the linked contact" },
                    "tags": { "type": "array", "items": { "type": "string" }, "description": "Tags for categorization" }
                }),
                vec!["title"],
            ),
            output_schema: None,
            annotations: None,
            execution: None,
            icons: None,
            meta: None,
        },
        Tool {
            name: "search_notes".into(),
            title: None,
            description: Some("Full-text search on note title and content with optional tag filter. Supports pagination via limit/offset.".into()),
            input_schema: make_schema(
                serde_json::json!({
                    "query": { "type": "string", "description": "Search query (full-text search on title + content)" },
                    "tag": { "type": "string", "description": "Filter to notes containing this tag" },
                    "limit": { "type": "integer", "description": "Max results (default 50, max 200)" },
                    "offset": { "type": "integer", "description": "Number of results to skip (default 0)" }
                }),
                vec!["query"],
            ),
            output_schema: None,
            annotations: None,
            execution: None,
            icons: None,
            meta: None,
        },
        Tool {
            name: "get_note".into(),
            title: None,
            description: Some("Get a specific note by ID".into()),
            input_schema: make_schema(
                serde_json::json!({
                    "id": { "type": "string", "description": "Note ID" }
                }),
                vec!["id"],
            ),
            output_schema: None,
            annotations: None,
            execution: None,
            icons: None,
            meta: None,
        },
        Tool {
            name: "update_note".into(),
            title: None,
            description: Some("Update a note's content, title, or tags".into()),
            input_schema: make_schema(
                serde_json::json!({
                    "id": { "type": "string", "description": "Note ID" },
                    "title": { "type": "string", "description": "New title" },
                    "content": { "type": "string", "description": "New content" },
                    "tags": { "type": "array", "items": { "type": "string" }, "description": "New tags (replaces existing)" }
                }),
                vec!["id"],
            ),
            output_schema: None,
            annotations: None,
            execution: None,
            icons: None,
            meta: None,
        },
        Tool {
            name: "delete_note".into(),
            title: None,
            description: Some("Delete a note by ID".into()),
            input_schema: make_schema(
                serde_json::json!({
                    "id": { "type": "string", "description": "Note ID" }
                }),
                vec!["id"],
            ),
            output_schema: None,
            annotations: None,
            execution: None,
            icons: None,
            meta: None,
        },
        Tool {
            name: "notes_by_contact".into(),
            title: None,
            description: Some("Get all notes linked to a contact email. Supports pagination via limit/offset.".into()),
            input_schema: make_schema(
                serde_json::json!({
                    "contact_email": { "type": "string", "description": "Contact email address" },
                    "limit": { "type": "integer", "description": "Max results (default 50, max 200)" },
                    "offset": { "type": "integer", "description": "Number of results to skip (default 0)" }
                }),
                vec!["contact_email"],
            ),
            output_schema: None,
            annotations: None,
            execution: None,
            icons: None,
            meta: None,
        },
        Tool {
            name: "recent_notes".into(),
            title: None,
            description: Some("Get the most recent notes across all types. Supports pagination via limit/offset.".into()),
            input_schema: make_schema(
                serde_json::json!({
                    "limit": { "type": "integer", "description": "Number of notes to return (default 50, max 200)" },
                    "offset": { "type": "integer", "description": "Number of notes to skip (default 0)" }
                }),
                vec![],
            ),
            output_schema: None,
            annotations: None,
            execution: None,
            icons: None,
            meta: None,
        },
        Tool {
            name: "note_stats".into(),
            title: None,
            description: Some("Get note counts grouped by type".into()),
            input_schema: empty_schema(),
            output_schema: None,
            annotations: None,
            execution: None,
            icons: None,
            meta: None,
        },
    ]
}

// ============================================================================
// MCP Server
// ============================================================================

#[derive(Clone)]
pub struct NotesMcpServer {
    db: Database,
}

impl NotesMcpServer {
    pub fn new(db: Database) -> Self {
        Self { db }
    }

    // ---- Tool handlers ----

    async fn handle_create_note(&self, args: &serde_json::Value) -> CallToolResult {
        let title = match get_str(args, "title") {
            Some(t) => t.trim().to_string(),
            None => return error_result("Missing required parameter: title"),
        };
        if title.is_empty() {
            return error_result("Parameter 'title' must not be empty");
        }
        if title.len() > MAX_TITLE_LEN {
            return error_result(&format!(
                "Parameter 'title' exceeds max length of {} chars",
                MAX_TITLE_LEN
            ));
        }

        let note_type = get_str(args, "note_type")
            .map(|s| s.trim().to_string())
            .unwrap_or_else(|| "internal".into());
        if !VALID_NOTE_TYPES.contains(&note_type.as_str()) {
            return error_result(&format!(
                "Invalid note_type '{}'. Must be one of: {}",
                note_type,
                VALID_NOTE_TYPES.join(", ")
            ));
        }

        let content = get_str(args, "content")
            .map(|s| s.trim().to_string())
            .unwrap_or_default();
        if content.len() > MAX_CONTENT_LEN {
            return error_result(&format!(
                "Parameter 'content' exceeds max length of {} chars",
                MAX_CONTENT_LEN
            ));
        }

        let contact_email = get_str(args, "contact_email")
            .map(|s| s.trim().to_string())
            .unwrap_or_default();
        if !contact_email.is_empty() && !is_valid_email(&contact_email) {
            return error_result(&format!(
                "Invalid email format for contact_email: '{}'",
                contact_email
            ));
        }

        let raw_tags = get_str_array(args, "tags");
        let tags = match validate_tags(&raw_tags) {
            Ok(t) => t,
            Err(e) => return error_result(&e),
        };

        let id = uuid::Uuid::new_v4().to_string();

        match sqlx::query_as::<_, Note>(
            "INSERT INTO notes.notes (id, note_type, title, content, contact_email, tags) VALUES ($1, $2, $3, $4, $5, $6) RETURNING *",
        )
        .bind(&id)
        .bind(&note_type)
        .bind(&title)
        .bind(&content)
        .bind(&contact_email)
        .bind(&tags)
        .fetch_one(self.db.pool())
        .await
        {
            Ok(note) => {
                info!(id = %id, title = %title, note_type = %note_type, "Created note");
                json_result(&note)
            }
            Err(e) => {
                error!(error = %e, title = %title, "Failed to create note");
                error_result(&format!("Failed to create note: {e}"))
            }
        }
    }

    async fn handle_search_notes(&self, args: &serde_json::Value) -> CallToolResult {
        let query = match get_str(args, "query") {
            Some(q) => q.trim().to_string(),
            None => return error_result("Missing required parameter: query"),
        };
        if query.is_empty() {
            return error_result("Parameter 'query' must not be empty");
        }

        let tag = get_str(args, "tag").map(|s| s.trim().to_string());
        let limit = clamp_limit(get_i64(args, "limit"), DEFAULT_LIMIT);
        let offset = clamp_offset(get_i64(args, "offset"));

        // Sanitize FTS query: keep only alphanumeric words, join with &
        let tsquery: String = query
            .split_whitespace()
            .filter(|w| w.chars().all(|c| c.is_alphanumeric()))
            .collect::<Vec<_>>()
            .join(" & ");

        if tsquery.is_empty() {
            return json_result(&Vec::<Note>::new());
        }

        let notes: Vec<Note> = if let Some(ref tag_val) = tag {
            if tag_val.is_empty() {
                return error_result("Parameter 'tag' must not be empty when provided");
            }
            match sqlx::query_as::<_, Note>(
                r#"SELECT * FROM notes.notes
                   WHERE to_tsvector('english', coalesce(title, '') || ' ' || coalesce(content, ''))
                         @@ to_tsquery('english', $1)
                     AND $2 = ANY(tags)
                   ORDER BY created_at DESC
                   LIMIT $3 OFFSET $4"#,
            )
            .bind(&tsquery)
            .bind(tag_val)
            .bind(limit)
            .bind(offset)
            .fetch_all(self.db.pool())
            .await
            {
                Ok(n) => n,
                Err(e) => {
                    error!(error = %e, query = %query, tag = %tag_val, "Search failed");
                    return error_result(&format!("Search failed: {e}"));
                }
            }
        } else {
            match sqlx::query_as::<_, Note>(
                r#"SELECT * FROM notes.notes
                   WHERE to_tsvector('english', coalesce(title, '') || ' ' || coalesce(content, ''))
                         @@ to_tsquery('english', $1)
                   ORDER BY created_at DESC
                   LIMIT $2 OFFSET $3"#,
            )
            .bind(&tsquery)
            .bind(limit)
            .bind(offset)
            .fetch_all(self.db.pool())
            .await
            {
                Ok(n) => n,
                Err(e) => {
                    error!(error = %e, query = %query, "Search failed");
                    return error_result(&format!("Search failed: {e}"));
                }
            }
        };

        json_result(&notes)
    }

    async fn handle_get_note(&self, id: &str) -> CallToolResult {
        let id = id.trim();
        if id.is_empty() {
            return error_result("Parameter 'id' must not be empty");
        }

        match sqlx::query_as::<_, Note>("SELECT * FROM notes.notes WHERE id = $1")
            .bind(id)
            .fetch_optional(self.db.pool())
            .await
        {
            Ok(Some(note)) => json_result(&note),
            Ok(None) => error_result(&format!("Note '{id}' not found")),
            Err(e) => {
                error!(error = %e, id = %id, "Failed to get note");
                error_result(&format!("Database error: {e}"))
            }
        }
    }

    async fn handle_update_note(&self, args: &serde_json::Value) -> CallToolResult {
        let id = match get_str(args, "id") {
            Some(i) => i.trim().to_string(),
            None => return error_result("Missing required parameter: id"),
        };
        if id.is_empty() {
            return error_result("Parameter 'id' must not be empty");
        }

        // Check that at least one update field is provided
        let has_title = args.get("title").is_some();
        let has_content = args.get("content").is_some();
        let has_tags = args.get("tags").is_some();
        if !has_title && !has_content && !has_tags {
            return error_result(
                "At least one of 'title', 'content', or 'tags' must be provided for update",
            );
        }

        let existing: Option<Note> = match sqlx::query_as("SELECT * FROM notes.notes WHERE id = $1")
            .bind(&id)
            .fetch_optional(self.db.pool())
            .await
        {
            Ok(n) => n,
            Err(e) => {
                error!(error = %e, id = %id, "Failed to fetch note for update");
                return error_result(&format!("Database error: {e}"));
            }
        };

        let existing = match existing {
            Some(n) => n,
            None => return error_result(&format!("Note '{id}' not found")),
        };

        let title = get_str(args, "title")
            .map(|s| s.trim().to_string())
            .unwrap_or(existing.title);
        if title.is_empty() {
            return error_result("Parameter 'title' must not be empty");
        }
        if title.len() > MAX_TITLE_LEN {
            return error_result(&format!(
                "Parameter 'title' exceeds max length of {} chars",
                MAX_TITLE_LEN
            ));
        }

        let content = get_str(args, "content")
            .map(|s| s.trim().to_string())
            .unwrap_or(existing.content);
        if content.len() > MAX_CONTENT_LEN {
            return error_result(&format!(
                "Parameter 'content' exceeds max length of {} chars",
                MAX_CONTENT_LEN
            ));
        }

        let tags = if has_tags {
            let raw_tags = get_str_array(args, "tags");
            match validate_tags(&raw_tags) {
                Ok(t) => t,
                Err(e) => return error_result(&e),
            }
        } else {
            existing.tags
        };

        match sqlx::query_as::<_, Note>(
            "UPDATE notes.notes SET title = $1, content = $2, tags = $3, updated_at = now() WHERE id = $4 RETURNING *",
        )
        .bind(&title)
        .bind(&content)
        .bind(&tags)
        .bind(&id)
        .fetch_one(self.db.pool())
        .await
        {
            Ok(note) => {
                info!(id = %id, "Updated note");
                json_result(&note)
            }
            Err(e) => {
                error!(error = %e, id = %id, "Failed to update note");
                error_result(&format!("Failed to update note: {e}"))
            }
        }
    }

    async fn handle_delete_note(&self, id: &str) -> CallToolResult {
        let id = id.trim();
        if id.is_empty() {
            return error_result("Parameter 'id' must not be empty");
        }

        match sqlx::query("DELETE FROM notes.notes WHERE id = $1")
            .bind(id)
            .execute(self.db.pool())
            .await
        {
            Ok(r) => {
                if r.rows_affected() > 0 {
                    info!(id = %id, "Deleted note");
                    json_result(&serde_json::json!({ "deleted": true, "id": id }))
                } else {
                    warn!(id = %id, "Attempted to delete non-existent note");
                    error_result(&format!("Note '{id}' not found"))
                }
            }
            Err(e) => {
                error!(error = %e, id = %id, "Failed to delete note");
                error_result(&format!("Failed to delete note: {e}"))
            }
        }
    }

    async fn handle_notes_by_contact(&self, args: &serde_json::Value) -> CallToolResult {
        let contact_email = match get_str(args, "contact_email") {
            Some(e) => e.trim().to_string(),
            None => return error_result("Missing required parameter: contact_email"),
        };
        if contact_email.is_empty() {
            return error_result("Parameter 'contact_email' must not be empty");
        }
        if !is_valid_email(&contact_email) {
            return error_result(&format!(
                "Invalid email format for contact_email: '{}'",
                contact_email
            ));
        }

        let limit = clamp_limit(get_i64(args, "limit"), DEFAULT_LIMIT);
        let offset = clamp_offset(get_i64(args, "offset"));

        match sqlx::query_as::<_, Note>(
            "SELECT * FROM notes.notes WHERE contact_email = $1 ORDER BY created_at DESC LIMIT $2 OFFSET $3",
        )
        .bind(&contact_email)
        .bind(limit)
        .bind(offset)
        .fetch_all(self.db.pool())
        .await
        {
            Ok(notes) => json_result(&notes),
            Err(e) => {
                error!(error = %e, contact_email = %contact_email, "Failed to fetch notes by contact");
                error_result(&format!("Database error: {e}"))
            }
        }
    }

    async fn handle_recent_notes(&self, args: &serde_json::Value) -> CallToolResult {
        let limit = clamp_limit(get_i64(args, "limit"), DEFAULT_LIMIT);
        let offset = clamp_offset(get_i64(args, "offset"));

        match sqlx::query_as::<_, Note>(
            "SELECT * FROM notes.notes ORDER BY created_at DESC LIMIT $1 OFFSET $2",
        )
        .bind(limit)
        .bind(offset)
        .fetch_all(self.db.pool())
        .await
        {
            Ok(notes) => json_result(&notes),
            Err(e) => {
                error!(error = %e, "Failed to fetch recent notes");
                error_result(&format!("Database error: {e}"))
            }
        }
    }

    async fn handle_note_stats(&self) -> CallToolResult {
        #[derive(sqlx::FromRow)]
        struct TypeCount {
            note_type: String,
            count: i64,
        }

        let rows: Vec<TypeCount> = match sqlx::query_as::<_, TypeCount>(
            "SELECT note_type, COUNT(*)::bigint as count FROM notes.notes GROUP BY note_type",
        )
        .fetch_all(self.db.pool())
        .await
        {
            Ok(r) => r,
            Err(e) => {
                error!(error = %e, "Failed to fetch note stats");
                return error_result(&format!("Database error: {e}"));
            }
        };

        let mut stats = NoteStats {
            total: 0,
            meeting: 0,
            call: 0,
            research: 0,
            internal: 0,
        };

        for row in &rows {
            match row.note_type.as_str() {
                "meeting" => stats.meeting = row.count,
                "call" => stats.call = row.count,
                "research" => stats.research = row.count,
                "internal" => stats.internal = row.count,
                _ => {}
            }
            stats.total += row.count;
        }

        json_result(&stats)
    }
}

// ============================================================================
// ServerHandler trait implementation
// ============================================================================

impl ServerHandler for NotesMcpServer {
    fn get_info(&self) -> ServerInfo {
        ServerInfo {
            protocol_version: ProtocolVersion::V_2024_11_05,
            capabilities: ServerCapabilities::builder().enable_tools().build(),
            server_info: Implementation::from_build_env(),
            instructions: Some(
                "DataXLR8 Notes MCP — create, search, and manage notes linked to contacts".into(),
            ),
        }
    }

    fn list_tools(
        &self,
        _request: Option<PaginatedRequestParams>,
        _context: RequestContext<RoleServer>,
    ) -> impl std::future::Future<Output = Result<ListToolsResult, rmcp::ErrorData>> + Send + '_ {
        async {
            Ok(ListToolsResult {
                tools: build_tools(),
                next_cursor: None,
                meta: None,
            })
        }
    }

    fn call_tool(
        &self,
        request: CallToolRequestParams,
        _context: RequestContext<RoleServer>,
    ) -> impl std::future::Future<Output = Result<CallToolResult, rmcp::ErrorData>> + Send + '_ {
        async move {
            let args =
                serde_json::to_value(&request.arguments).unwrap_or(serde_json::Value::Null);
            let name_str: &str = request.name.as_ref();

            let result = match name_str {
                "create_note" => self.handle_create_note(&args).await,
                "search_notes" => self.handle_search_notes(&args).await,
                "get_note" => match get_str(&args, "id") {
                    Some(id) => {
                        let id = id.trim().to_string();
                        if id.is_empty() {
                            error_result("Parameter 'id' must not be empty")
                        } else {
                            self.handle_get_note(&id).await
                        }
                    }
                    None => error_result("Missing required parameter: id"),
                },
                "update_note" => self.handle_update_note(&args).await,
                "delete_note" => match get_str(&args, "id") {
                    Some(id) => {
                        let id = id.trim().to_string();
                        if id.is_empty() {
                            error_result("Parameter 'id' must not be empty")
                        } else {
                            self.handle_delete_note(&id).await
                        }
                    }
                    None => error_result("Missing required parameter: id"),
                },
                "notes_by_contact" => self.handle_notes_by_contact(&args).await,
                "recent_notes" => self.handle_recent_notes(&args).await,
                "note_stats" => self.handle_note_stats().await,
                _ => {
                    warn!(tool = %request.name, "Unknown tool called");
                    error_result(&format!("Unknown tool: {}", request.name))
                }
            };

            Ok(result)
        }
    }
}
