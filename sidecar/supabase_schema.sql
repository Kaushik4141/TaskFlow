-- =====================================================================
-- TaskFlow Supabase Schema: Vault Notes, Embeddings & RLS
-- Run this in your Supabase Dashboard -> SQL Editor
-- =====================================================================

-- 1. Enable pgvector extension for AI semantic vector similarity search
CREATE EXTENSION IF NOT EXISTS vector;

-- 2. Create vault_notes table for synchronizing Obsidian notes
CREATE TABLE IF NOT EXISTS vault_notes (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    user_id UUID REFERENCES auth.users(id) ON DELETE CASCADE,
    
    -- File metadata
    path TEXT NOT NULL,                  -- Relative path, e.g. "TaskFlow/Projects/engine.md"
    title TEXT NOT NULL,                 -- Note title
    content TEXT NOT NULL,               -- Full Markdown body
    metadata JSONB DEFAULT '{}'::jsonb,  -- Frontmatter tags, dates, project slug
    
    -- Vector embedding (384 dimensions matching TaskFlow's all-MiniLM-L6-v2)
    embedding vector(384),
    
    -- Hash for change detection
    file_hash TEXT NOT NULL,
    updated_at TIMESTAMPTZ DEFAULT NOW(),
    
    -- Uniqueness per user and file path
    UNIQUE(user_id, path)
);

-- 3. Enable Row-Level Security (RLS) for complete multi-user data isolation
ALTER TABLE vault_notes ENABLE ROW LEVEL SECURITY;

-- 4. RLS Policy: Users (and their AI agent tokens) can ONLY access their own notes
DROP POLICY IF EXISTS "Users can only access their own notes" ON vault_notes;
CREATE POLICY "Users can only access their own notes"
ON vault_notes
FOR ALL
TO authenticated
USING (auth.uid() = user_id)
WITH CHECK (auth.uid() = user_id);

-- 5. Service Role bypass policy (for automated background sync scripts using service key)
DROP POLICY IF EXISTS "Service role full access" ON vault_notes;
CREATE POLICY "Service role full access"
ON vault_notes
FOR ALL
TO service_role
USING (true)
WITH CHECK (true);

-- 6. Cosine similarity index for high-speed vector retrieval
CREATE INDEX IF NOT EXISTS vault_notes_embedding_idx 
ON vault_notes 
USING ivfflat (embedding vector_cosine_ops)
WITH (lists = 100);

-- 7. Stored Procedure for Semantic Vector Similarity Search
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
SECURITY INVOKER -- Inherits the caller's RLS permissions automatically
AS $$
BEGIN
    RETURN QUERY
    SELECT
        vault_notes.id,
        vault_notes.path,
        vault_notes.title,
        vault_notes.content,
        vault_notes.metadata,
        1 - (vault_notes.embedding <=> query_embedding) AS similarity
    FROM vault_notes
    WHERE (1 - (vault_notes.embedding <=> query_embedding)) >= match_threshold
      AND (filter_prefix = '' OR vault_notes.path LIKE filter_prefix || '%')
    ORDER BY vault_notes.embedding <=> query_embedding
    LIMIT match_count;
END;
$$;
