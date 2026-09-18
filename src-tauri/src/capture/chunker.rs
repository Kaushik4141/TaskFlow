use super::types::ContentType;

#[allow(dead_code)]
#[derive(Debug, Clone)]
pub struct ContentChunk {
    pub text: String,
    pub chunk_type: String,
    pub position: usize,
}

pub fn chunk_content(
    content: &str,
    content_type: &ContentType,
    max_chunk_size: usize,
) -> Vec<ContentChunk> {
    let max_chunk_size = max_chunk_size.max(120);
    match content_type {
        ContentType::CodeContent => chunk_code(content, max_chunk_size),
        ContentType::TerminalContent => chunk_terminal(content, max_chunk_size),
        ContentType::BrowserContent => chunk_browser(content, max_chunk_size),
        _ => chunk_generic(content, max_chunk_size),
    }
}

fn chunk_code(content: &str, max: usize) -> Vec<ContentChunk> {
    let mut chunks = Vec::new();
    let mut current = String::new();
    let mut position = 0usize;
    let mut current_position = 0usize;

    for line in content.lines() {
        let trimmed = line.trim_start();
        let boundary = [
            "fn ",
            "def ",
            "class ",
            "function ",
            "const ",
            "let ",
            "var ",
            "async ",
            "export ",
            "import ",
        ]
        .iter()
        .any(|prefix| trimmed.starts_with(prefix));

        if boundary && !current.trim().is_empty() {
            push_split(&mut chunks, &current, "code_block", current_position, max);
            current.clear();
            current_position = position;
        }

        current.push_str(line);
        current.push('\n');
        if current.len() >= max {
            push_split(&mut chunks, &current, "code_block", current_position, max);
            current.clear();
            current_position = position + line.len();
        }
        position += line.len() + 1;
    }

    if !current.trim().is_empty() {
        push_split(&mut chunks, &current, "code_block", current_position, max);
    }
    chunks
}

fn chunk_terminal(content: &str, max: usize) -> Vec<ContentChunk> {
    let mut chunks = Vec::new();
    for (index, line) in content.lines().enumerate() {
        if looks_like_command(line) {
            push_split(&mut chunks, line, "command", index, max);
        }
    }
    if chunks.is_empty() {
        chunk_generic(content, max)
    } else {
        chunks
    }
}

fn chunk_browser(content: &str, max: usize) -> Vec<ContentChunk> {
    let mut chunks = Vec::new();
    let mut position = 0usize;
    for paragraph in content.split("\n\n") {
        let trimmed = paragraph.trim();
        if trimmed.is_empty() {
            position += paragraph.len() + 2;
            continue;
        }
        let chunk_type = if trimmed.starts_with("http://") || trimmed.starts_with("https://") {
            "url"
        } else if trimmed.len() < 90 && !trimmed.ends_with('.') {
            "heading"
        } else {
            "paragraph"
        };
        push_split(&mut chunks, trimmed, chunk_type, position, max);
        position += paragraph.len() + 2;
    }
    chunks
}

fn chunk_generic(content: &str, max: usize) -> Vec<ContentChunk> {
    let mut chunks = Vec::new();
    let mut current = String::new();
    let mut position = 0usize;
    let mut current_position = 0usize;

    for line in content.lines() {
        if current.is_empty() {
            current_position = position;
        }
        if current.len() + line.len() + 1 > max && !current.trim().is_empty() {
            push_split(&mut chunks, &current, "paragraph", current_position, max);
            current.clear();
            current_position = position;
        }
        current.push_str(line);
        current.push('\n');
        position += line.len() + 1;
    }

    if !current.trim().is_empty() {
        push_split(&mut chunks, &current, "paragraph", current_position, max);
    }
    chunks
}

fn push_split(
    chunks: &mut Vec<ContentChunk>,
    text: &str,
    chunk_type: &str,
    position: usize,
    max: usize,
) {
    let trimmed = text.trim();
    if trimmed.is_empty() {
        return;
    }
    let mut offset = 0usize;
    let chars = trimmed.chars().collect::<Vec<_>>();
    while offset < chars.len() {
        let end = (offset + max).min(chars.len());
        let chunk = chars[offset..end].iter().collect::<String>();
        chunks.push(ContentChunk {
            text: chunk,
            chunk_type: chunk_type.to_string(),
            position: position + offset,
        });
        offset = end;
    }
}

fn looks_like_command(line: &str) -> bool {
    let trimmed = line.trim();
    ["$ ", "> ", "# ", "❯ ", "→ "]
        .iter()
        .any(|prompt| trimmed.starts_with(prompt))
        || trimmed.starts_with("npm ")
        || trimmed.starts_with("git ")
        || trimmed.starts_with("cargo ")
        || trimmed.starts_with("pnpm ")
        || trimmed.starts_with("python ")
}
