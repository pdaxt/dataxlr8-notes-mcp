# dataxlr8-notes-mcp

MCP server for creating, searching, and managing notes linked to contacts. Supports full-text search, tagging, and categorization by note type (meeting, call, research, internal).

## Tools

| Tool | Description |
|------|-------------|
| create_note | Create a new note linked to a contact |
| search_notes | Full-text search on note title and content with optional tag filter. Supports pagination via limit/offset. |
| get_note | Get a specific note by ID |
| update_note | Update a note's content, title, or tags |
| delete_note | Delete a note by ID |
| notes_by_contact | Get all notes linked to a contact email. Supports pagination via limit/offset. |
| recent_notes | Get the most recent notes across all types. Supports pagination via limit/offset. |
| note_stats | Get note counts grouped by type |

## Setup

```bash
DATABASE_URL=postgres://dataxlr8:dataxlr8@localhost:5432/dataxlr8 cargo run
```

## Schema

Creates `notes.*` schema in PostgreSQL with tables:
- `notes.notes` - Note records with full-text search support

## Part of

[DataXLR8](https://github.com/pdaxt) - AI-powered recruitment platform
