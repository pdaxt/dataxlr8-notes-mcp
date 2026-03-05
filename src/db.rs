use anyhow::Result;
use sqlx::PgPool;

pub async fn setup_schema(pool: &PgPool) -> Result<()> {
    sqlx::raw_sql(
        r#"
        CREATE SCHEMA IF NOT EXISTS notes;

        CREATE TABLE IF NOT EXISTS notes.notes (
            id            TEXT PRIMARY KEY,
            note_type     TEXT NOT NULL DEFAULT 'internal'
                          CHECK (note_type IN ('meeting', 'call', 'research', 'internal')),
            title         TEXT NOT NULL,
            content       TEXT NOT NULL DEFAULT '',
            contact_email TEXT NOT NULL DEFAULT '',
            tags          TEXT[] NOT NULL DEFAULT '{}',
            created_at    TIMESTAMPTZ NOT NULL DEFAULT now(),
            updated_at    TIMESTAMPTZ NOT NULL DEFAULT now()
        );

        CREATE INDEX IF NOT EXISTS idx_notes_contact_email ON notes.notes(contact_email);
        CREATE INDEX IF NOT EXISTS idx_notes_note_type ON notes.notes(note_type);
        CREATE INDEX IF NOT EXISTS idx_notes_created_at ON notes.notes(created_at DESC);
        CREATE INDEX IF NOT EXISTS idx_notes_tags ON notes.notes USING GIN(tags);
        "#,
    )
    .execute(pool)
    .await?;

    // FTS index: separate statement to avoid issues with raw_sql batching
    sqlx::raw_sql(
        r#"
        DO $$
        BEGIN
            IF NOT EXISTS (
                SELECT 1 FROM pg_indexes
                WHERE schemaname = 'notes'
                  AND indexname = 'idx_notes_fts'
            ) THEN
                CREATE INDEX idx_notes_fts ON notes.notes
                    USING GIN (to_tsvector('english', coalesce(title, '') || ' ' || coalesce(content, '')));
            END IF;
        END $$;
        "#,
    )
    .execute(pool)
    .await?;

    Ok(())
}
