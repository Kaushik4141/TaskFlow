-- =====================================================================
-- TaskFlow 24/7 Cloud Brain Schema for Supabase (pgvector + RLS)
-- Enables cloud agents (Hermes, OpenClaw, remote LLMs) to query your memory
-- even when your local laptop is asleep or powered off.
--
-- Instructions:
-- 1. Open your Supabase Project Dashboard -> SQL Editor
-- 2. Paste and run this entire script
-- =====================================================================

-- 1. Enable pgvector extension for sub-15ms semantic vector similarity search
CREATE EXTENSION IF NOT EXISTS vector;

-- =====================================================================
-- Table 1: vault_notes (Markdown Vault Mirror)
-- =====================================================================
CREATE TABLE IF NOT EXISTS vault_notes (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    user_id UUID REFERENCES auth.users(id) ON DELETE CASCADE,
    
    path TEXT NOT NULL,                  -- Relative path: "TaskFlow/Projects/TaskFlow.md"
    title TEXT NOT NULL,                 -- Note title
    content TEXT NOT NULL,               -- Full Markdown body
    metadata JSONB DEFAULT '{}'::jsonb,  -- Frontmatter tags, project slug, dates
    
    embedding vector(384),               -- 384 dimensions matching all-MiniLM-L6-v2
    file_hash TEXT NOT NULL,             -- SHA256 hash for incremental delta syncing
    updated_at TIMESTAMPTZ DEFAULT NOW(),
    
    UNIQUE(user_id, path)
);

ALTER TABLE vault_notes ENABLE ROW LEVEL SECURITY;

DROP POLICY IF EXISTS "Users can access their own notes" ON vault_notes;
CREATE POLICY "Users can access their own notes"
ON vault_notes FOR ALL TO authenticated
USING (auth.uid() = user_id) WITH CHECK (auth.uid() = user_id);

DROP POLICY IF EXISTS "Service role full access on vault_notes" ON vault_notes;
CREATE POLICY "Service role full access on vault_notes"
ON vault_notes FOR ALL TO service_role
USING (true) WITH CHECK (true);

DROP POLICY IF EXISTS "Anon client access on vault_notes" ON vault_notes;
CREATE POLICY "Anon client access on vault_notes"
ON vault_notes FOR ALL TO anon
USING (true) WITH CHECK (true);

CREATE INDEX IF NOT EXISTS vault_notes_path_idx ON vault_notes(path);
CREATE INDEX IF NOT EXISTS vault_notes_embedding_idx 
ON vault_notes USING ivfflat (embedding vector_cosine_ops) WITH (lists = 100);

-- =====================================================================
-- Table 2: cloud_rollups (Atomic 10-Minute Activity Memory Units)
-- =====================================================================
CREATE TABLE IF NOT EXISTS cloud_rollups (
    id TEXT PRIMARY KEY,                 -- Rollup UUID from local SQLite
    user_id UUID REFERENCES auth.users(id) ON DELETE CASCADE,
    
    project_slug TEXT,                   -- e.g. "AuthService", "TaskFlow"
    window_start TIMESTAMPTZ NOT NULL,
    window_end TIMESTAMPTZ NOT NULL,
    title TEXT NOT NULL,
    summary_md TEXT NOT NULL,
    key_points JSONB DEFAULT '[]'::jsonb,
    apps JSONB DEFAULT '[]'::jsonb,
    resources JSONB DEFAULT '[]'::jsonb,
    
    embedding vector(384),               -- Semantic vector embedding of title + summary
    created_at TIMESTAMPTZ DEFAULT NOW()
);

ALTER TABLE cloud_rollups ENABLE ROW LEVEL SECURITY;

DROP POLICY IF EXISTS "Users can access their own rollups" ON cloud_rollups;
CREATE POLICY "Users can access their own rollups"
ON cloud_rollups FOR ALL TO authenticated
USING (auth.uid() = user_id) WITH CHECK (auth.uid() = user_id);

DROP POLICY IF EXISTS "Service role full access on cloud_rollups" ON cloud_rollups;
CREATE POLICY "Service role full access on cloud_rollups"
ON cloud_rollups FOR ALL TO service_role
USING (true) WITH CHECK (true);

DROP POLICY IF EXISTS "Anon client access on cloud_rollups" ON cloud_rollups;
CREATE POLICY "Anon client access on cloud_rollups"
ON cloud_rollups FOR ALL TO anon
USING (true) WITH CHECK (true);

CREATE INDEX IF NOT EXISTS cloud_rollups_project_idx ON cloud_rollups(project_slug);
CREATE INDEX IF NOT EXISTS cloud_rollups_window_idx ON cloud_rollups(window_start, window_end);
CREATE INDEX IF NOT EXISTS cloud_rollups_embedding_idx 
ON cloud_rollups USING ivfflat (embedding vector_cosine_ops) WITH (lists = 100);

-- =====================================================================
-- Table 3: cloud_graph_edges (Knowledge Graph Entity Relationships)
-- =====================================================================
CREATE TABLE IF NOT EXISTS cloud_graph_edges (
    id BIGINT GENERATED ALWAYS AS IDENTITY PRIMARY KEY,
    user_id UUID REFERENCES auth.users(id) ON DELETE CASCADE,
    
    source_entity TEXT NOT NULL,         -- "Projects/TaskFlow"
    target_entity TEXT NOT NULL,         -- "Apps/cursor", "Sites/github.com"
    relation_type TEXT NOT NULL,         -- "used_tool", "visited_site", "active_on"
    weight FLOAT DEFAULT 1.0,
    time_bucket TEXT NOT NULL,           -- "YYYY-MM" (e.g. "2026-09")
    last_seen TIMESTAMPTZ NOT NULL,
    
    UNIQUE(user_id, source_entity, target_entity, relation_type, time_bucket)
);

ALTER TABLE cloud_graph_edges ENABLE ROW LEVEL SECURITY;

DROP POLICY IF EXISTS "Users can access their own graph edges" ON cloud_graph_edges;
CREATE POLICY "Users can access their own graph edges"
ON cloud_graph_edges FOR ALL TO authenticated
USING (auth.uid() = user_id) WITH CHECK (auth.uid() = user_id);

DROP POLICY IF EXISTS "Service role full access on cloud_graph_edges" ON cloud_graph_edges;
CREATE POLICY "Service role full access on cloud_graph_edges"
ON cloud_graph_edges FOR ALL TO service_role
USING (true) WITH CHECK (true);

DROP POLICY IF EXISTS "Anon client access on cloud_graph_edges" ON cloud_graph_edges;
CREATE POLICY "Anon client access on cloud_graph_edges"
ON cloud_graph_edges FOR ALL TO anon
USING (true) WITH CHECK (true);

CREATE INDEX IF NOT EXISTS cloud_graph_edges_source_idx ON cloud_graph_edges(source_entity, time_bucket, weight DESC);

-- =====================================================================
-- Stored Procedure 1: Scope-First GraphRAG Cloud Retrieval
-- =====================================================================
CREATE OR REPLACE FUNCTION query_cloud_memory(
    query_embedding vector(384) DEFAULT NULL,
    filter_project text DEFAULT NULL,
    filter_time_bucket text DEFAULT NULL,
    match_limit int DEFAULT 5
)
RETURNS TABLE (
    id TEXT,
    project_slug TEXT,
    window_start TIMESTAMPTZ,
    window_end TIMESTAMPTZ,
    title TEXT,
    summary_md TEXT,
    apps JSONB,
    resources JSONB,
    similarity float
)
LANGUAGE plpgsql
SECURITY INVOKER
AS $$
BEGIN
    RETURN QUERY
    SELECT
        cr.id,
        cr.project_slug,
        cr.window_start,
        cr.window_end,
        cr.title,
        cr.summary_md,
        cr.apps,
        cr.resources,
        CASE
            WHEN query_embedding IS NOT NULL AND cr.embedding IS NOT NULL
            THEN 1 - (cr.embedding <=> query_embedding)
            ELSE 1.0
        END AS similarity
    FROM cloud_rollups cr
    WHERE (filter_project IS NULL OR cr.project_slug ILIKE filter_project)
      AND (filter_time_bucket IS NULL OR to_char(cr.window_end, 'YYYY-MM') = filter_time_bucket)
    ORDER BY
        CASE
            WHEN query_embedding IS NOT NULL AND cr.embedding IS NOT NULL
            THEN cr.embedding <=> query_embedding
            ELSE 0
        END ASC,
        cr.window_start DESC
    LIMIT match_limit;
END;
$$;

-- =====================================================================
-- Stored Procedure 2: Full Vault Note Search
-- =====================================================================
CREATE OR REPLACE FUNCTION search_vault_notes(
    query_embedding vector(384),
    match_threshold float DEFAULT 0.1,
    match_count int DEFAULT 5,
    filter_prefix text DEFAULT ''
)
RETURNS TABLE (
    id UUID,
    path TEXT,
    title TEXT,
    content TEXT,
    metadata JSONB,
    similarity float
)
LANGUAGE plpgsql
SECURITY INVOKER
AS $$
BEGIN
    RETURN QUERY
    SELECT
        vn.id,
        vn.path,
        vn.title,
        vn.content,
        vn.metadata,
        1 - (vn.embedding <=> query_embedding) AS similarity
    FROM vault_notes vn
    WHERE (1 - (vn.embedding <=> query_embedding)) >= match_threshold
      AND (filter_prefix = '' OR vn.path LIKE filter_prefix || '%')
    ORDER BY vn.embedding <=> query_embedding
    LIMIT match_count;
END;
$$;
