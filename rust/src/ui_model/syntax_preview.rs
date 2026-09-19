use std::ops::Range;
use std::path::Path;

const MAX_SPANS: usize = 65_536;
const MAX_SPAN_BYTES: usize = 1024 * 1024;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SyntaxLanguage {
    Rust,
    Python,
    JavaScript,
    TypeScript,
    Json,
    Toml,
    Yaml,
    Markdown,
    Shell,
    PowerShell,
    C,
    Cpp,
    Html,
}

impl SyntaxLanguage {
    pub fn for_path(path: &Path) -> Option<Self> {
        let extension = path.extension()?.to_str()?;
        if extension == "C" {
            return Some(Self::Cpp);
        }
        match extension.to_ascii_lowercase().as_str() {
            "rs" => Some(Self::Rust),
            "py" | "pyw" => Some(Self::Python),
            "js" | "mjs" | "cjs" | "jsx" => Some(Self::JavaScript),
            "ts" | "mts" | "cts" | "tsx" => Some(Self::TypeScript),
            "json" => Some(Self::Json),
            "toml" => Some(Self::Toml),
            "yaml" | "yml" => Some(Self::Yaml),
            "md" | "markdown" => Some(Self::Markdown),
            "sh" | "bash" | "zsh" => Some(Self::Shell),
            "ps1" | "psm1" | "psd1" => Some(Self::PowerShell),
            "c" | "h" => Some(Self::C),
            "cc" | "cpp" | "cxx" | "hh" | "hpp" | "hxx" => Some(Self::Cpp),
            "html" | "htm" => Some(Self::Html),
            _ => None,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SyntaxTokenKind {
    Keyword,
    String,
    Comment,
    Number,
    Heading,
    Tag,
    Attribute,
    Preprocessor,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SyntaxSpan {
    pub range: Range<u32>,
    pub kind: SyntaxTokenKind,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Mode {
    Normal,
    BlockComment,
    HtmlComment,
    Quoted(u8),
    Triple(u8),
    RawCpp,
}

#[derive(Clone, Debug)]
pub struct SyntaxHighlight {
    language: SyntaxLanguage,
    spans: Vec<SyntaxSpan>,
    parsed_bytes: usize,
    mode: Mode,
    fallback: bool,
}

impl SyntaxHighlight {
    pub fn new(language: SyntaxLanguage) -> Self {
        Self {
            language,
            spans: Vec::new(),
            parsed_bytes: 0,
            mode: Mode::Normal,
            fallback: false,
        }
    }

    pub fn language(&self) -> SyntaxLanguage {
        self.language
    }
    pub fn spans(&self) -> &[SyntaxSpan] {
        &self.spans
    }
    pub fn is_plain_fallback(&self) -> bool {
        self.fallback
    }
    pub fn capacity_bytes(&self) -> usize {
        self.spans.capacity() * std::mem::size_of::<SyntaxSpan>()
    }

    pub fn append(&mut self, body: &str, canceled: &dyn Fn() -> bool) {
        if self.fallback || self.parsed_bytes >= body.len() {
            return;
        }
        let bytes = body.as_bytes();
        let mut i = self.parsed_bytes;
        while i < bytes.len() {
            if i.is_multiple_of(4096) && canceled() {
                return;
            }
            let start = i;
            match self.mode {
                Mode::BlockComment | Mode::HtmlComment => {
                    let end = if self.mode == Mode::BlockComment {
                        b"*/".as_slice()
                    } else {
                        b"-->".as_slice()
                    };
                    while i < bytes.len() && !bytes[i..].starts_with(end) {
                        if i.is_multiple_of(4096) && canceled() {
                            return;
                        }
                        i += 1;
                    }
                    if i < bytes.len() {
                        i += end.len();
                        self.mode = Mode::Normal;
                    }
                    self.push(start..i, SyntaxTokenKind::Comment);
                }
                Mode::Quoted(quote) | Mode::Triple(quote) => {
                    let triple = matches!(self.mode, Mode::Triple(_));
                    while i < bytes.len() {
                        if i.is_multiple_of(4096) && canceled() {
                            return;
                        }
                        if !triple && bytes[i] == b'\\' {
                            i = (i + 2).min(bytes.len());
                            continue;
                        }
                        if triple && bytes[i..].starts_with(&[quote, quote, quote]) {
                            i += 3;
                            self.mode = Mode::Normal;
                            break;
                        }
                        if !triple && bytes[i] == quote {
                            i += 1;
                            self.mode = Mode::Normal;
                            break;
                        }
                        i += 1;
                    }
                    self.push(start..i, SyntaxTokenKind::String);
                }
                Mode::RawCpp => {
                    while i < bytes.len() && !bytes[i..].starts_with(b")\"") {
                        if i.is_multiple_of(4096) && canceled() {
                            return;
                        }
                        i += 1;
                    }
                    if i < bytes.len() {
                        i += 2;
                        self.mode = Mode::Normal;
                    }
                    self.push(start..i, SyntaxTokenKind::String);
                }
                Mode::Normal => {
                    let line_start = i == 0 || bytes[i - 1] == b'\n';
                    let language = self.language;
                    if language == SyntaxLanguage::Html && bytes[i..].starts_with(b"<!--") {
                        self.mode = Mode::HtmlComment;
                        i += 4;
                        self.push(start..i, SyntaxTokenKind::Comment);
                    } else if supports_block_comment(language) && bytes[i..].starts_with(b"/*") {
                        self.mode = Mode::BlockComment;
                        i += 2;
                        self.push(start..i, SyntaxTokenKind::Comment);
                    } else if is_line_comment(language, &bytes[i..]) {
                        i = bytes[i..]
                            .iter()
                            .position(|b| *b == b'\n')
                            .map_or(bytes.len(), |n| i + n);
                        self.push(start..i, SyntaxTokenKind::Comment);
                    } else if is_preprocessor(language) && line_start && bytes[i] == b'#' {
                        i = bytes[i..]
                            .iter()
                            .position(|b| *b == b'\n')
                            .map_or(bytes.len(), |n| i + n);
                        self.push(start..i, SyntaxTokenKind::Preprocessor);
                    } else if language == SyntaxLanguage::Markdown && line_start && bytes[i] == b'#'
                    {
                        i = bytes[i..]
                            .iter()
                            .position(|b| *b == b'\n')
                            .map_or(bytes.len(), |n| i + n);
                        self.push(start..i, SyntaxTokenKind::Heading);
                    } else if language == SyntaxLanguage::Html
                        && bytes[i] == b'<'
                        && bytes
                            .get(i + 1)
                            .is_some_and(|b| b.is_ascii_alphabetic() || matches!(*b, b'/' | b'!'))
                    {
                        i += 1;
                        let mut quote = None;
                        while i < bytes.len() {
                            if i.is_multiple_of(4096) && canceled() {
                                return;
                            }
                            let b = bytes[i];
                            if quote == Some(b) {
                                quote = None;
                            } else if quote.is_none() && matches!(b, b'\'' | b'"') {
                                quote = Some(b);
                            } else if quote.is_none() && b == b'>' {
                                i += 1;
                                break;
                            }
                            i += 1;
                        }
                        self.push(start..i, SyntaxTokenKind::Tag);
                    } else if language == SyntaxLanguage::Cpp && bytes[i..].starts_with(b"R\"(") {
                        self.mode = Mode::RawCpp;
                        i += 3;
                        self.push(start..i, SyntaxTokenKind::String);
                    } else if is_quote(language, bytes[i]) {
                        let quote = bytes[i];
                        if language == SyntaxLanguage::Python
                            && bytes[i..].starts_with(&[quote, quote, quote])
                        {
                            self.mode = Mode::Triple(quote);
                            i += 3;
                        } else {
                            self.mode = Mode::Quoted(quote);
                            i += 1;
                        }
                        self.push(start..i, SyntaxTokenKind::String);
                    } else if bytes[i].is_ascii_digit() && (i == 0 || !is_word(bytes[i - 1])) {
                        i += 1;
                        while i < bytes.len()
                            && (bytes[i].is_ascii_alphanumeric()
                                || bytes[i] == b'.'
                                || bytes[i] == b'_')
                        {
                            i += 1;
                        }
                        self.push(start..i, SyntaxTokenKind::Number);
                    } else if is_word(bytes[i]) && (i == 0 || !is_word(bytes[i - 1])) {
                        i += 1;
                        while i < bytes.len() && is_word(bytes[i]) {
                            i += 1;
                        }
                        let word = &body[start..i];
                        let next = bytes[i..].iter().position(|b| *b != b' ').map(|n| i + n);
                        let kind = if is_keyword(language, word) {
                            Some(SyntaxTokenKind::Keyword)
                        } else if matches!(language, SyntaxLanguage::Yaml | SyntaxLanguage::Toml)
                            && next
                                .and_then(|at| bytes.get(at))
                                .is_some_and(|b| matches!(*b, b':' | b'='))
                        {
                            Some(SyntaxTokenKind::Attribute)
                        } else {
                            None
                        };
                        if let Some(kind) = kind {
                            self.push(start..i, kind);
                        }
                    } else {
                        i += 1;
                    }
                }
            }
            if self.fallback {
                break;
            }
        }
        self.parsed_bytes = body.len();
    }

    fn push(&mut self, range: Range<usize>, kind: SyntaxTokenKind) {
        if range.is_empty() || self.fallback {
            return;
        }
        if let Some(last) = self.spans.last_mut() {
            if last.kind == kind && last.range.end as usize == range.start {
                last.range.end = range.end as u32;
                return;
            }
        }
        if self.spans.len() >= MAX_SPANS
            || (self.spans.len() + 1) * std::mem::size_of::<SyntaxSpan>() > MAX_SPAN_BYTES
        {
            self.spans.clear();
            self.spans.shrink_to_fit();
            self.fallback = true;
            return;
        }
        self.spans.push(SyntaxSpan {
            range: range.start as u32..range.end as u32,
            kind,
        });
    }
}

fn is_word(b: u8) -> bool {
    b.is_ascii_alphanumeric() || b == b'_'
}
fn supports_block_comment(l: SyntaxLanguage) -> bool {
    matches!(
        l,
        SyntaxLanguage::Rust
            | SyntaxLanguage::JavaScript
            | SyntaxLanguage::TypeScript
            | SyntaxLanguage::C
            | SyntaxLanguage::Cpp
    )
}
fn is_preprocessor(l: SyntaxLanguage) -> bool {
    matches!(l, SyntaxLanguage::C | SyntaxLanguage::Cpp)
}
fn is_line_comment(l: SyntaxLanguage, tail: &[u8]) -> bool {
    match l {
        SyntaxLanguage::Rust
        | SyntaxLanguage::JavaScript
        | SyntaxLanguage::TypeScript
        | SyntaxLanguage::C
        | SyntaxLanguage::Cpp => tail.starts_with(b"//"),
        SyntaxLanguage::Python
        | SyntaxLanguage::Toml
        | SyntaxLanguage::Yaml
        | SyntaxLanguage::Shell
        | SyntaxLanguage::PowerShell => tail.starts_with(b"#"),
        _ => false,
    }
}
fn is_quote(l: SyntaxLanguage, b: u8) -> bool {
    match l {
        SyntaxLanguage::Json => b == b'"',
        SyntaxLanguage::Rust | SyntaxLanguage::C | SyntaxLanguage::Cpp => matches!(b, b'"' | b'\''),
        SyntaxLanguage::JavaScript | SyntaxLanguage::TypeScript => matches!(b, b'"' | b'\'' | b'`'),
        SyntaxLanguage::Html | SyntaxLanguage::Markdown => false,
        _ => matches!(b, b'"' | b'\''),
    }
}
fn is_keyword(l: SyntaxLanguage, word: &str) -> bool {
    let words: &[&str] = match l {
        SyntaxLanguage::Rust => &[
            "fn", "let", "mut", "pub", "struct", "enum", "impl", "use", "mod", "match", "if",
            "else", "return", "async", "await", "trait", "const",
        ],
        SyntaxLanguage::Python => &[
            "def", "class", "import", "from", "if", "elif", "else", "return", "async", "await",
            "for", "in", "while", "with", "as", "True", "False", "None",
        ],
        SyntaxLanguage::JavaScript | SyntaxLanguage::TypeScript => &[
            "function",
            "const",
            "let",
            "var",
            "class",
            "interface",
            "type",
            "export",
            "import",
            "from",
            "return",
            "async",
            "await",
            "if",
            "else",
            "true",
            "false",
            "null",
        ],
        SyntaxLanguage::Json | SyntaxLanguage::Yaml | SyntaxLanguage::Toml => {
            &["true", "false", "null"]
        }
        SyntaxLanguage::Shell => &[
            "if", "then", "else", "fi", "for", "in", "do", "done", "case", "esac", "function",
        ],
        SyntaxLanguage::PowerShell => &[
            "function", "param", "if", "else", "foreach", "in", "return", "switch", "true", "false",
        ],
        SyntaxLanguage::C | SyntaxLanguage::Cpp => &[
            "int",
            "char",
            "void",
            "static",
            "const",
            "return",
            "if",
            "else",
            "struct",
            "class",
            "namespace",
            "template",
            "typename",
            "auto",
            "include",
            "using",
            "bool",
        ],
        SyntaxLanguage::Markdown | SyntaxLanguage::Html => &[],
    };
    words.contains(&word)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn detects_every_approved_language_and_exact_uppercase_c() {
        for (name, language) in [
            ("x.rs", SyntaxLanguage::Rust),
            ("x.py", SyntaxLanguage::Python),
            ("x.js", SyntaxLanguage::JavaScript),
            ("x.tsx", SyntaxLanguage::TypeScript),
            ("x.json", SyntaxLanguage::Json),
            ("x.toml", SyntaxLanguage::Toml),
            ("x.yml", SyntaxLanguage::Yaml),
            ("x.md", SyntaxLanguage::Markdown),
            ("x.sh", SyntaxLanguage::Shell),
            ("x.ps1", SyntaxLanguage::PowerShell),
            ("x.c", SyntaxLanguage::C),
            ("x.h", SyntaxLanguage::C),
            ("x.C", SyntaxLanguage::Cpp),
            ("x.cpp", SyntaxLanguage::Cpp),
            ("x.html", SyntaxLanguage::Html),
        ] {
            assert_eq!(
                SyntaxLanguage::for_path(Path::new(name)),
                Some(language),
                "{name}"
            );
        }
        assert_eq!(SyntaxLanguage::for_path(Path::new("x.unknown")), None);
    }

    #[test]
    fn classifies_c_cpp_html_and_multiline_across_append() {
        for (language, text, kind) in [
            (
                SyntaxLanguage::C,
                "#include <x>\nint value; /* comment */\n",
                SyntaxTokenKind::Preprocessor,
            ),
            (
                SyntaxLanguage::Cpp,
                "template<class T>\nR\"(text)\"\n",
                SyntaxTokenKind::Keyword,
            ),
            (
                SyntaxLanguage::Html,
                "<p class=\"x\">hello</p><!-- note -->",
                SyntaxTokenKind::Tag,
            ),
        ] {
            let mut syntax = SyntaxHighlight::new(language);
            syntax.append(text, &|| false);
            assert!(
                syntax.spans().iter().any(|span| span.kind == kind),
                "{language:?}"
            );
        }
        let mut syntax = SyntaxHighlight::new(SyntaxLanguage::C);
        syntax.append("/* first\n", &|| false);
        syntax.append("/* first\nsecond */ int x;", &|| false);
        assert!(syntax
            .spans()
            .iter()
            .any(|span| span.kind == SyntaxTokenKind::Comment && span.range.end as usize >= 18));
    }

    #[test]
    fn classifies_each_supported_language_without_modifying_text() {
        for (language, source, expected) in [
            (
                SyntaxLanguage::Rust,
                "fn main() { let n = 1; }",
                SyntaxTokenKind::Keyword,
            ),
            (
                SyntaxLanguage::Python,
                "def greet():\n    return 'hi'",
                SyntaxTokenKind::Keyword,
            ),
            (
                SyntaxLanguage::JavaScript,
                "const x = `hello`;",
                SyntaxTokenKind::String,
            ),
            (
                SyntaxLanguage::TypeScript,
                "interface User { name: string }",
                SyntaxTokenKind::Keyword,
            ),
            (
                SyntaxLanguage::Json,
                "{\"key\": true}",
                SyntaxTokenKind::String,
            ),
            (
                SyntaxLanguage::Toml,
                "name = 'value'",
                SyntaxTokenKind::Attribute,
            ),
            (
                SyntaxLanguage::Yaml,
                "name: value",
                SyntaxTokenKind::Attribute,
            ),
            (
                SyntaxLanguage::Markdown,
                "# Heading\nbody",
                SyntaxTokenKind::Heading,
            ),
            (
                SyntaxLanguage::Shell,
                "if true; then echo hi; fi",
                SyntaxTokenKind::Keyword,
            ),
            (
                SyntaxLanguage::PowerShell,
                "function Get-Thing { return 1 }",
                SyntaxTokenKind::Keyword,
            ),
            (
                SyntaxLanguage::C,
                "int main(void) { return 0; }",
                SyntaxTokenKind::Keyword,
            ),
            (
                SyntaxLanguage::Cpp,
                "template<class T> class Box {};",
                SyntaxTokenKind::Keyword,
            ),
            (
                SyntaxLanguage::Html,
                "<div class='box'>x</div>",
                SyntaxTokenKind::Tag,
            ),
        ] {
            let mut syntax = SyntaxHighlight::new(language);
            syntax.append(source, &|| false);
            assert!(
                syntax.spans().iter().any(|span| span.kind == expected),
                "{language:?}"
            );
            assert!(!syntax.is_plain_fallback());
            assert!(syntax
                .spans()
                .iter()
                .all(|span| span.range.end as usize <= source.len()));
        }
    }

    #[test]
    fn bounded_spans_fall_back_to_plain_without_losing_text() {
        let source = "let x = 1;\n".repeat(40_000);
        let mut syntax = SyntaxHighlight::new(SyntaxLanguage::Rust);
        syntax.append(&source, &|| false);
        assert!(syntax.is_plain_fallback());
        assert!(syntax.spans().is_empty());
        assert!(syntax.capacity_bytes() <= MAX_SPAN_BYTES);
    }
}
