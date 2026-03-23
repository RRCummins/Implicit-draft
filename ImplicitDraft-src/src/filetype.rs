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
            "rs" | "py" | "pyi" | "pyw" | "js" | "ts" | "tsx" | "jsx" | "go" | "swift"
            | "swiftinterface" | "toml" | "yaml" | "yml" | "sh" | "bash" | "zsh" | "json" | "css"
            | "html" | "xml" | "c" | "h" | "cc" | "cpp" | "cxx" | "hpp" | "hxx" | "java" | "kt"
            | "kts" | "rb" | "php" | "m" | "mm",
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
        assert_eq!(detect(&PathBuf::from("main.kts")), FileType::Code);
        assert_eq!(detect(&PathBuf::from("main.hxx")), FileType::Code);
    }

    #[test]
    fn detects_text_files() {
        assert_eq!(detect(&PathBuf::from("todo.txt")), FileType::Text);
    }
}
