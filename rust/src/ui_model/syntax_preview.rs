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
    Csv,
    Tsv,
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
            "csv" => Some(Self::Csv),
            "tsv" => Some(Self::Tsv),
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
    Delimiter,
    Column(u8),
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
    DelimitedQuoted {
        delimiter: u8,
        column: u8,
    },
    DelimitedQuotedPending {
        delimiter: u8,
        column: u8,
    },
    DelimitedUnquoted {
        delimiter: u8,
        start: usize,
        column: u8,
    },
}

#[derive(Clone, Debug)]
pub struct SyntaxHighlight {
    language: SyntaxLanguage,
    spans: Vec<SyntaxSpan>,
    parsed_bytes: usize,
    mode: Mode,
    delimited_column: u8,
    fallback: bool,
}

impl SyntaxHighlight {
    pub fn new(language: SyntaxLanguage) -> Self {
        Self {
            language,
            spans: Vec::new(),
            parsed_bytes: 0,
            mode: Mode::Normal,
            delimited_column: 0,
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
                Mode::DelimitedQuoted { delimiter, column } => {
                    let mut closed = false;
                    while i < bytes.len() {
                        if i.is_multiple_of(4096) && canceled() {
                            return;
                        }
                        if bytes[i] == b'"' {
                            if i + 1 == bytes.len() {
                                self.mode = Mode::DelimitedQuotedPending { delimiter, column };
                                self.push(start..i, SyntaxTokenKind::Column(column));
                                self.parsed_bytes = i;
                                return;
                            }
                            if bytes.get(i + 1) == Some(&b'"') {
                                i += 2;
                                continue;
                            }
                            i += 1;
                            self.mode = Mode::Normal;
                            closed = true;
                            break;
                        }
                        i += 1;
                    }
                    self.push(start..i, SyntaxTokenKind::Column(column));
                    if closed && i < bytes.len() && bytes[i] == delimiter {
                        self.push(i..i + 1, SyntaxTokenKind::Delimiter);
                        i += 1;
                        self.delimited_column = next_delimited_column(column);
                    }
                }
                Mode::DelimitedQuotedPending { delimiter, column } => {
                    if i >= bytes.len() {
                        return;
                    }
                    if bytes.get(i + 1) == Some(&b'"') {
                        self.mode = Mode::DelimitedQuoted { delimiter, column };
                        i += 2;
                        self.push(start..i, SyntaxTokenKind::Column(column));
                    } else {
                        self.mode = Mode::Normal;
                        i += 1;
                        self.push(start..i, SyntaxTokenKind::Column(column));
                        if i < bytes.len() && bytes[i] == delimiter {
                            self.push(i..i + 1, SyntaxTokenKind::Delimiter);
                            i += 1;
                            self.delimited_column = next_delimited_column(column);
                        }
                    }
                }
                Mode::DelimitedUnquoted {
                    delimiter,
                    start,
                    column,
                } => {
                    while i < bytes.len() && bytes[i] != delimiter && bytes[i] != b'\n' {
                        if i.is_multiple_of(4096) && canceled() {
                            return;
                        }
                        i += 1;
                    }
                    if i == bytes.len() {
                        break;
                    }
                    self.push(start..i, SyntaxTokenKind::Column(column));
                    self.mode = Mode::Normal;
                    if bytes[i] == delimiter {
                        self.push(i..i + 1, SyntaxTokenKind::Delimiter);
                        i += 1;
                        self.delimited_column = next_delimited_column(column);
                    } else {
                        self.delimited_column = 0;
                    }
                }
                Mode::Normal => {
                    let line_start = i == 0 || bytes[i - 1] == b'\n';
                    let language = self.language;
                    if let Some(delimiter) = delimited_separator(language) {
                        if bytes[i] == b'\n' {
                            self.delimited_column = 0;
                            i += 1;
                        } else if bytes[i] == delimiter {
                            self.push(i..i + 1, SyntaxTokenKind::Delimiter);
                            i += 1;
                            self.delimited_column = next_delimited_column(self.delimited_column);
                        } else if is_delimited_field_start(bytes, i, delimiter) && bytes[i] == b'"'
                        {
                            let column = self.delimited_column;
                            self.mode = Mode::DelimitedQuoted { delimiter, column };
                            i += 1;
                            self.push(start..i, SyntaxTokenKind::Column(column));
                        } else if is_delimited_field_start(bytes, i, delimiter) {
                            self.mode = Mode::DelimitedUnquoted {
                                delimiter,
                                start: i,
                                column: self.delimited_column,
                            };
                            i += 1;
                        } else {
                            i += 1;
                        }
                    } else if language == SyntaxLanguage::Html && bytes[i..].starts_with(b"<!--") {
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

    pub fn finish(&mut self, body: &str) {
        let bytes = body.as_bytes();
        match self.mode {
            Mode::DelimitedQuotedPending { delimiter, column } => {
                let i = self.parsed_bytes;
                if i < bytes.len() && bytes[i] == b'"' {
                    self.mode = Mode::Normal;
                    self.push(i..i + 1, SyntaxTokenKind::Column(column));
                    self.parsed_bytes = i + 1;
                    if i + 1 < bytes.len() && bytes[i + 1] == delimiter {
                        self.push(i + 1..i + 2, SyntaxTokenKind::Delimiter);
                        self.parsed_bytes = i + 2;
                        self.delimited_column = next_delimited_column(column);
                    }
                }
            }
            Mode::DelimitedUnquoted { start, column, .. } if start < bytes.len() => {
                self.push(start..bytes.len(), SyntaxTokenKind::Column(column));
                self.mode = Mode::Normal;
                self.parsed_bytes = bytes.len();
            }
            _ => {}
        }
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
fn delimited_separator(l: SyntaxLanguage) -> Option<u8> {
    match l {
        SyntaxLanguage::Csv => Some(b','),
        SyntaxLanguage::Tsv => Some(b'\t'),
        _ => None,
    }
}
fn next_delimited_column(column: u8) -> u8 {
    (column + 1) % 8
}
fn is_delimited_field_start(bytes: &[u8], index: usize, delimiter: u8) -> bool {
    index == 0 || bytes[index - 1] == delimiter || bytes[index - 1] == b'\n'
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
        SyntaxLanguage::Markdown
        | SyntaxLanguage::Html
        | SyntaxLanguage::Csv
        | SyntaxLanguage::Tsv => &[],
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
            ("x.csv", SyntaxLanguage::Csv),
            ("x.tsv", SyntaxLanguage::Tsv),
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
    fn colors_csv_and_tsv_fields_by_column_without_reformatting_text() {
        let mut csv = SyntaxHighlight::new(SyntaxLanguage::Csv);
        let csv_source = "name,age,note\nAlice,42,\"hello, world\"\nempty,\n";
        csv.append(csv_source, &|| false);
        assert!(csv
            .spans()
            .iter()
            .any(|span| span.kind == SyntaxTokenKind::Delimiter));
        assert!(csv
            .spans()
            .iter()
            .any(|span| span.kind == SyntaxTokenKind::Column(1)
                && &csv_source[span.range.start as usize..span.range.end as usize] == "42"));
        assert!(csv
            .spans()
            .iter()
            .any(|span| span.kind == SyntaxTokenKind::Column(0)
                && &csv_source[span.range.start as usize..span.range.end as usize] == "Alice"));
        assert!(csv
            .spans()
            .iter()
            .any(|span| span.kind == SyntaxTokenKind::Column(1)
                && &csv_source[span.range.start as usize..span.range.end as usize] == "age"));
        assert!(csv
            .spans()
            .iter()
            .any(|span| span.kind == SyntaxTokenKind::Column(2)
                && &csv_source[span.range.start as usize..span.range.end as usize]
                    == "\"hello, world\""));

        let mut tsv = SyntaxHighlight::new(SyntaxLanguage::Tsv);
        let tsv_source = "name\tvalue\nitem\t3.14\n";
        tsv.append(tsv_source, &|| false);
        assert!(tsv
            .spans()
            .iter()
            .any(|span| span.kind == SyntaxTokenKind::Delimiter));
        assert!(tsv
            .spans()
            .iter()
            .any(|span| span.kind == SyntaxTokenKind::Column(1)
                && &tsv_source[span.range.start as usize..span.range.end as usize] == "3.14"));
        assert!(tsv
            .spans()
            .iter()
            .any(|span| span.kind == SyntaxTokenKind::Column(0)
                && &tsv_source[span.range.start as usize..span.range.end as usize] == "item"));
    }

    #[test]
    fn preserves_quoted_csv_state_across_pages_and_escaped_quotes() {
        let mut syntax = SyntaxHighlight::new(SyntaxLanguage::Csv);
        let first = "id,note\n1,\"line one\n";
        let complete = "id,note\n1,\"line one\nline two with \"\"quote\"\"\"\n";
        syntax.append(first, &|| false);
        syntax.append(complete, &|| false);

        let quoted = syntax
            .spans()
            .iter()
            .find(|span| {
                span.kind == SyntaxTokenKind::Column(1)
                    && complete[span.range.start as usize..span.range.end as usize].starts_with('"')
            })
            .expect("quoted CSV field span");
        assert_eq!(
            &complete[quoted.range.start as usize..quoted.range.end as usize],
            "\"line one\nline two with \"\"quote\"\"\""
        );
    }

    #[test]
    fn preserves_csv_escaped_quote_when_page_ends_between_quote_bytes() {
        let mut syntax = SyntaxHighlight::new(SyntaxLanguage::Csv);
        let first = "id,note\n1,\"hello \"";
        let complete = "id,note\n1,\"hello \"\"world\"\n";
        syntax.append(first, &|| false);
        syntax.append(complete, &|| false);

        let quoted = syntax
            .spans()
            .iter()
            .find(|span| {
                span.kind == SyntaxTokenKind::Column(1)
                    && complete[span.range.start as usize..span.range.end as usize].starts_with('"')
            })
            .expect("quoted CSV field span");
        assert_eq!(
            &complete[quoted.range.start as usize..quoted.range.end as usize],
            "\"hello \"\"world\""
        );

        let mut eof = SyntaxHighlight::new(SyntaxLanguage::Csv);
        let eof_source = "id,note\n1,\"x\"";
        eof.append(eof_source, &|| false);
        eof.finish(eof_source);
        assert!(eof.spans().iter().any(|span| {
            span.kind == SyntaxTokenKind::Column(1)
                && &eof_source[span.range.start as usize..span.range.end as usize] == "\"x\""
        }));
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
