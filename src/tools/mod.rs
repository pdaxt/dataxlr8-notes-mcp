use dataxlr8_mcp_core::mcp::{empty_schema, error_result, get_i64, get_str, get_str_array, json_result, make_schema};
use dataxlr8_mcp_core::Database;
use rmcp::model::*;
use rmcp::service::{RequestContext, RoleServer};
use rmcp::ServerHandler;
use serde::{Deserialize, Serialize};
use tracing::{error, info};

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
            description: Some("Full-text search on note title and content with optional tag filter".into()),
            input_schema: make_schema(
                serde_json::json!({
                    "query": { "type": "string", "description": "Search query (full-text search on title + content)" },
                    "tag": { "type": "string", "description": "Filter to notes containing this tag" },
                    "limit": { "type": "integer", "description": "Max results (default 20)" }
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
            description: Some("Get all notes linked to a contact email".into()),
            input_schema: make_schema(
                serde_json::json!({
                    "contact_email": { "type": "string", "description": "Contact email address" }
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
            description: Some("Get the most recent notes across all types".into()),
            input_schema: make_schema(
                serde_json::json!({
                    "limit": { "type": "integer", "description": "Number of notes to return (default 10, max 100)" }
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
            Some(t) => t,
            None => return error_result("Missing required parameter: title"),
        };
        let note_type = get_str(args, "note_type").unwrap_or_else(|| "internal".into());
        let content = get_str(args, "content").unwrap_or_default();
        let contact_email = get_str(args, "contact_email").unwrap_or_default();
        let tags = get_str_array(args, "tags");

        if !["meeting", "call", "research", "internal"].contains(&note_type.as_str()) {
            return error_result("note_type must be one of: meeting, call, research, internal");
        }

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
                info!(id = id, title = title, "Created note");
                json_result(&note)
            }
            Err(e) => error_result(&format!("Failed to create note: {e}")),
        }
    }

    async fn handle_search_notes(&self, args: &serde_json::Value) -> CallToolResult {
        let query = match get_str(args, "query") {
            Some(q) => q,
            None => return error_result("Missing required parameter: query"),
        };
        let tag = get_str(args, "tag");
        let limit = get_i64(args, "limit").unwrap_or(20).min(100);

        // Sanitize FTS query: keep only alphanumeric words, join with &
        let tsquery: String = query
            .split_whitespace()
            .filter(|w| w.chars().all(|c| c.is_alphanumeric()))
            .collect::<Vec<_>>()
            .join(" & ");

        if tsquery.is_empty() {
            return json_result(&Vec::<Note>::new());
        }

        let notes: Vec<Note> = if let Some(tag_val) = tag {
            match sqlx::query_as::<_, Note>(
                r#"SELECT * FROM notes.notes
                   WHERE to_tsvector('english', coalesce(title, '') || ' ' || coalesce(content, ''))
                         @@ to_tsquery('english', $1)
                     AND $2 = ANY(tags)
                   ORDER BY created_at DESC
                   LIMIT $3"#,
            )
            .bind(&tsquery)
            .bind(&tag_val)
            .bind(limit)
            .fetch_all(self.db.pool())
            .await
            {
                Ok(n) => n,
                Err(e) => return error_result(&format!("Search failed: {e}")),
            }
        } else {
            match sqlx::query_as::<_, Note>(
                r#"SELECT * FROM notes.notes
                   WHERE to_tsvector('english', coalesce(title, '') || ' ' || coalesce(content, ''))
                         @@ to_tsquery('english', $1)
                   ORDER BY created_at DESC
                   LIMIT $2"#,
            )
            .bind(&tsquery)
            .bind(limit)
            .fetch_all(self.db.pool())
            .await
            {
                Ok(n) => n,
                Err(e) => return error_result(&format!("Search failed: {e}")),
            }
        };

        json_result(&notes)
    }

    async fn handle_get_note(&self, id: &str) -> CallToolResult {
        match sqlx::query_as::<_, Note>("SELECT * FROM notes.notes WHERE id = $1")
            .bind(id)
            .fetch_optional(self.db.pool())
            .await
        {
            Ok(Some(note)) => json_result(&note),
            Ok(None) => error_result(&format!("Note '{id}' not found")),
            Err(e) => error_result(&format!("Database error: {e}")),
        }
    }

    async fn handle_update_note(&self, args: &serde_json::Value) -> CallToolResult {
        let id = match get_str(args, "id") {
            Some(i) => i,
            None => return error_result("Missing required parameter: id"),
        };

        let existing: Option<Note> = match sqlx::query_as("SELECT * FROM notes.notes WHERE id = $1")
            .bind(&id)
            .fetch_optional(self.db.pool())
            .await
        {
            Ok(n) => n,
            Err(e) => return error_result(&format!("Database error: {e}")),
        };

        let existing = match existing {
            Some(n) => n,
            None => return error_result(&format!("Note '{id}' not found")),
        };

        let title = get_str(args, "title").unwrap_or(existing.title);
        let content = get_str(args, "content").unwrap_or(existing.content);
        let tags_arg = args.get("tags");
        let tags = if tags_arg.is_some() {
            get_str_array(args, "tags")
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
                info!(id = id, "Updated note");
                json_result(&note)
            }
            Err(e) => error_result(&format!("Failed to update note: {e}")),
        }
    }

    async fn handle_delete_note(&self, id: &str) -> CallToolResult {
        match sqlx::query("DELETE FROM notes.notes WHERE id = $1")
            .bind(id)
            .execute(self.db.pool())
            .await
        {
            Ok(r) => {
                if r.rows_affected() > 0 {
                    info!(id = id, "Deleted note");
                    json_result(&serde_json::json!({ "deleted": true, "id": id }))
                } else {
                    error_result(&format!("Note '{id}' not found"))
                }
            }
            Err(e) => error_result(&format!("Failed to delete note: {e}")),
        }
    }

    async fn handle_notes_by_contact(&self, contact_email: &str) -> CallToolResult {
        match sqlx::query_as::<_, Note>(
            "SELECT * FROM notes.notes WHERE contact_email = $1 ORDER BY created_at DESC",
        )
        .bind(contact_email)
        .fetch_all(self.db.pool())
        .await
        {
            Ok(notes) => json_result(&notes),
            Err(e) => error_result(&format!("Database error: {e}")),
        }
    }

    async fn handle_recent_notes(&self, limit: i64) -> CallToolResult {
        let limit = limit.clamp(1, 100);
        match sqlx::query_as::<_, Note>(
            "SELECT * FROM notes.notes ORDER BY created_at DESC LIMIT $1",
        )
        .bind(limit)
        .fetch_all(self.db.pool())
        .await
        {
            Ok(notes) => json_result(&notes),
            Err(e) => error_result(&format!("Database error: {e}")),
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
            Err(e) => return error_result(&format!("Database error: {e}")),
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
                    Some(id) => self.handle_get_note(&id).await,
                    None => error_result("Missing required parameter: id"),
                },
                "update_note" => self.handle_update_note(&args).await,
                "delete_note" => match get_str(&args, "id") {
                    Some(id) => self.handle_delete_note(&id).await,
                    None => error_result("Missing required parameter: id"),
                },
                "notes_by_contact" => match get_str(&args, "contact_email") {
                    Some(email) => self.handle_notes_by_contact(&email).await,
                    None => error_result("Missing required parameter: contact_email"),
                },
                "recent_notes" => {
                    let limit = get_i64(&args, "limit").unwrap_or(10);
                    self.handle_recent_notes(limit).await
                }
                "note_stats" => self.handle_note_stats().await,
                _ => error_result(&format!("Unknown tool: {}", request.name)),
            };

            Ok(result)
        }
    }
}
