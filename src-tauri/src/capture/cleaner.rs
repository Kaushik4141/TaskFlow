use regex::Regex;

pub fn clean_captured_content(raw: &str) -> String {
    let cleaned: String = raw
        .chars()
        .filter(|c| {
            let cp = *c as u32;
            cp != 0xFFFC
                && !(0xE000..=0xF8FF).contains(&cp)
                && !(0xFE00..=0xFE0F).contains(&cp)
                && (*c == '\n' || *c == '\t' || !c.is_control())
        })
        .collect();

    let re_spaces = Regex::new(r" {2,}").expect("valid spaces regex");
    let cleaned = re_spaces.replace_all(&cleaned, " ");

    let re_newlines = Regex::new(r"\n{3,}").expect("valid newlines regex");
    let cleaned = re_newlines.replace_all(&cleaned, "\n\n");

    cleaned
        .lines()
        .filter_map(|line| {
            let trimmed = line.trim();
            if trimmed.is_empty() {
                return None;
            }
            trimmed
                .chars()
                .any(|c| c.is_alphanumeric())
                .then(|| trimmed.to_string())
        })
        .collect::<Vec<_>>()
        .join("\n")
        .trim()
        .to_string()
}

pub fn is_meaningful_content(content: &str) -> bool {
    let cleaned = content.trim();
    if cleaned.len() < 30 {
        return false;
    }

    let letter_count = cleaned.chars().filter(|c| c.is_alphabetic()).count();
    let total = cleaned.chars().count().max(1);
    let ratio = letter_count as f32 / total as f32;
    ratio >= 0.30
}
