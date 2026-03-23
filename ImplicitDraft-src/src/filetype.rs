use std::path::Path;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum FileType {
    Markdown,
    Text,
    Code,
    Unknown,
}

pub fn detect(path: &Path) -> FileType {
    match path.extension().and_then(|ext| ext.to_str()) {
        Some("md" | "markdown") => FileType::Markdown,
        Some("txt") => FileType::Text,
        Some(
            "rs" | "py" | "js" | "ts" | "tsx" | "jsx" | "go" | "swift" | "toml" | "yaml" | "yml"
            | "sh" | "bash" | "zsh" | "json" | "css" | "html" | "c" | "h" | "cpp" | "hpp" | "java"
            | "kt" | "rb" | "php",
        ) => FileType::Code,
        Some(_) | None => FileType::Unknown,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    #[test]
    fn detects_markdown_files() {
        assert_eq!(detect(&PathBuf::from("note.md")), FileType::Markdown);
    }

    #[test]
    fn detects_code_files() {
        assert_eq!(detect(&PathBuf::from("main.rs")), FileType::Code);
        assert_eq!(detect(&PathBuf::from("config.toml")), FileType::Code);
    }

    #[test]
    fn detects_text_files() {
        assert_eq!(detect(&PathBuf::from("todo.txt")), FileType::Text);
    }
}
