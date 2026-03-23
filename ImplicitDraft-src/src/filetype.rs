use std::path::Path;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum FileType {
    Markdown,
    Text,
    Code,
    Unknown,
}

pub fn detect(path: &Path) -> FileType {
    if let Some(name) = file_name_lower(path)
        && matches!(
            name.as_str(),
            "dockerfile" | "makefile" | "justfile" | ".bashrc" | ".zshrc" | ".bash_profile"
        )
    {
        return FileType::Code;
    }

    match path
        .extension()
        .and_then(|ext| ext.to_str())
        .map(str::to_ascii_lowercase)
        .as_deref()
    {
        Some("md" | "markdown") => FileType::Markdown,
        Some("txt" | "text") => FileType::Text,
        Some(
            "rs" | "py" | "pyi" | "pyw" | "js" | "mjs" | "cjs" | "ts" | "tsx" | "jsx" | "go"
            | "swift" | "swiftinterface" | "toml" | "yaml" | "yml" | "ini" | "conf" | "sh" | "bash"
            | "zsh" | "fish" | "json" | "jsonc" | "css" | "scss" | "html" | "htm" | "xml" | "c"
            | "h" | "hh" | "cc" | "cpp" | "cxx" | "hpp" | "hxx" | "ipp" | "java" | "kt" | "kts"
            | "rb" | "php" | "m" | "mm" | "cs" | "scala" | "lua" | "dart" | "sql" | "r" | "zig",
        ) => FileType::Code,
        Some(_) | None => FileType::Unknown,
    }
}

pub fn is_supported(path: &Path) -> bool {
    !matches!(detect(path), FileType::Unknown)
}

fn file_name_lower(path: &Path) -> Option<String> {
    path.file_name()
        .and_then(|name| name.to_str())
        .map(str::to_ascii_lowercase)
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
        assert_eq!(detect(&PathBuf::from("App.swift")), FileType::Code);
        assert_eq!(detect(&PathBuf::from("ViewController.m")), FileType::Code);
        assert_eq!(detect(&PathBuf::from("Program.cs")), FileType::Code);
        assert_eq!(detect(&PathBuf::from("schema.sql")), FileType::Code);
        assert_eq!(detect(&PathBuf::from("Dockerfile")), FileType::Code);
        assert_eq!(detect(&PathBuf::from(".zshrc")), FileType::Code);
    }

    #[test]
    fn detects_text_files() {
        assert_eq!(detect(&PathBuf::from("todo.txt")), FileType::Text);
    }

    #[test]
    fn reports_supported_editable_files() {
        assert!(is_supported(Path::new("note.md")));
        assert!(is_supported(Path::new("main.swift")));
        assert!(is_supported(Path::new("Makefile")));
        assert!(!is_supported(Path::new("image.png")));
    }
}
