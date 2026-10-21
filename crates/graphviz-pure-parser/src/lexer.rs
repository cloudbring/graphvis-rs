//! Re-entrant DOT lexer matching Graphviz `lib/cgraph/scan.l`.
//!
//! Like the flex scanner it never fails. A character no rule matches is a token of its own
//! (scan.l's `.` rule returns `yytext[0]`), which the grammar then rejects, and end of input
//! inside a quoted string, an HTML string or a comment ends the token stream with the scanner
//! state kept for `aagerror`'s message.

use std::sync::Arc;

use graphviz_pure_core::Result;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TokenKind {
    Strict,
    Graph,
    Digraph,
    Subgraph,
    Node,
    Edge,
    /// `->`
    Arrow,
    /// `--`
    DashDash,
    /// `{`
    LBrace,
    /// `}`
    RBrace,
    /// `[`
    LBracket,
    /// `]`
    RBracket,
    /// `;`
    Semi,
    /// `,`
    Comma,
    /// `=`
    Equals,
    /// `:`
    Colon,
    /// `+`
    Plus,
    /// Unquoted identifier or numeric literal
    Ident(String),
    /// Double-quoted string literal `"..."`
    Quoted(String),
    /// HTML-like string literal `<...>` (stored with outer `<` and `>` delimiters)
    Html(String),
    /// Any other character (scan.l's `.` rule: `return aagget_text(yyscanner)[0]`). No
    /// grammar rule accepts it, so it is always a syntax error.
    Other(char),
    /// End of input, or `@` (scan.l's `GRAPH_EOF_TOKEN`, which returns `EOF`).
    Eof,
}

/// The scanner's start condition when the input ended (`YYSTATE` in scan.l's `aagerror`).
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub enum ScanState {
    #[default]
    Initial,
    /// Inside `"..."`, with the string read so far (`Sbuf`).
    QString(String),
    /// Inside `<...>`, with the string read so far, without the opening `<` (`Sbuf`).
    HString(String),
    /// Inside `/* ... */`.
    Comment,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Token {
    pub kind: TokenKind,
    /// Physical line (1-based) where the token starts.
    pub line: usize,
    pub col: usize,
    /// flex's `yytext` once the token is read, which `aagerror` prints after `near`: the
    /// source text, but `"` for a quoted string and `>` for an HTML string (the text of the
    /// last rule they matched), and empty at end of input.
    pub text: String,
    /// cgraph's `line_num` once the token is read: the newlines read so far, renumbered by
    /// `# N` line directives (`ppDirective`). `aagerror` reports this line, so an error is
    /// reported on the line where its lookahead token ends. An `int` in C, so it wraps.
    pub c_line: i32,
    /// `InputFile` as set by a `# N "file"` line directive, if any.
    pub file: Option<Arc<str>>,
    /// Scanner state at the end of input (only an [`TokenKind::Eof`] token has another).
    pub state: ScanState,
}

pub struct Lexer<'a> {
    input: &'a str,
    pos: usize,
    line: usize,
    col: usize,
    /// cgraph's `line_num`.
    c_line: i32,
    /// cgraph's `InputFile`, when a line directive named one.
    file: Option<Arc<str>>,
}

impl<'a> Lexer<'a> {
    pub fn new(input: &'a str) -> Self {
        let mut pos = 0;
        // Skip UTF-8 BOM if present at the beginning of input (matches scan.l)
        if input.as_bytes().starts_with(&[0xEF, 0xBB, 0xBF]) {
            pos = 3;
        }
        Self {
            input,
            pos,
            line: 1,
            col: 1,
            c_line: 1,
            file: None,
        }
    }

    pub fn tokenize_all(&mut self) -> Result<Vec<Token>> {
        let mut tokens = Vec::new();
        loop {
            let tok = self.next_token()?;
            let is_eof = tok.kind == TokenKind::Eof;
            tokens.push(tok);
            if is_eof {
                break;
            }
        }
        Ok(tokens)
    }

    fn peek_char(&self) -> Option<char> {
        self.input[self.pos..].chars().next()
    }

    fn peek_next_char(&self) -> Option<char> {
        let mut iter = self.input[self.pos..].chars();
        iter.next()?;
        iter.next()
    }

    fn advance_char(&mut self) -> Option<char> {
        let ch = self.peek_char()?;
        self.pos += ch.len_utf8();
        if ch == '\n' {
            self.line += 1;
            self.c_line = self.c_line.wrapping_add(1);
            self.col = 1;
        } else {
            self.col += 1;
        }
        Some(ch)
    }

    /// Consumes the rest of the line, not its newline (flex's `.*`), and returns it.
    fn skip_to_eol(&mut self) -> &'a str {
        let input = self.input;
        let start = self.pos;
        while let Some(ch) = self.peek_char() {
            if ch == '\n' {
                break;
            }
            self.advance_char();
        }
        &input[start..self.pos]
    }

    /// Skips whitespace and comments. Returns [`ScanState::Comment`] when the input ends
    /// inside a `/* ... */` comment.
    fn skip_whitespace_and_comments(&mut self) -> Option<ScanState> {
        loop {
            // Skip whitespace
            while let Some(ch) = self.peek_char() {
                if ch == ' ' || ch == '\t' || ch == '\r' || ch == '\n' {
                    self.advance_char();
                } else {
                    break;
                }
            }

            match (self.peek_char(), self.peek_next_char()) {
                // C++-style line comment `// ...`
                (Some('/'), Some('/')) => {
                    self.skip_to_eol();
                }
                // C-style block comment `/* ... */`
                (Some('/'), Some('*')) => {
                    self.advance_char();
                    self.advance_char();
                    let mut closed = false;
                    while let Some(ch) = self.advance_char() {
                        if ch == '*' && self.peek_char() == Some('/') {
                            self.advance_char();
                            closed = true;
                            break;
                        }
                    }
                    if !closed {
                        return Some(ScanState::Comment);
                    }
                }
                // `^"#".*` is a line directive (`ppDirective`), `"#".*` a comment.
                (Some('#'), _) => {
                    let at_bol = self.pos == 0 || self.input.as_bytes()[self.pos - 1] == b'\n';
                    let text = self.skip_to_eol();
                    if at_bol {
                        self.pp_directive(&text[1..]);
                    }
                }
                _ => return None,
            }
        }
    }

    /// scan.l `ppDirective`: `# N` or `#line N`, optionally followed by `"file"`, makes the
    /// next line line `N` (of `file`). `s` is the line after the `#`.
    fn pp_directive(&mut self, s: &str) {
        let s = s.strip_prefix("line").unwrap_or(s);
        // sscanf(s, "%d %1[\"]%n", ...)
        let s = s.trim_start_matches(|c: char| c.is_ascii_whitespace() || c == '\x0b');
        let (neg, digits) = match s.as_bytes().first() {
            Some(b'-') => (true, &s[1..]),
            Some(b'+') => (false, &s[1..]),
            _ => (false, s),
        };
        let n_digits = digits.bytes().take_while(u8::is_ascii_digit).count();
        if n_digits == 0 {
            return;
        }
        let magnitude = digits[..n_digits].bytes().fold(0i64, |acc, d| {
            acc.saturating_mul(10).saturating_add(i64::from(d - b'0'))
        });
        // `%d` into an `int`: the conversion saturates at 64 bits, the store keeps the low 32
        let lineno = if neg { -magnitude } else { magnitude } as i32;
        // ignore if line number was out of range
        if lineno <= 0 {
            return;
        }
        self.c_line = lineno - 1;
        let rest =
            digits[n_digits..].trim_start_matches(|c: char| c.is_ascii_whitespace() || c == '\x0b');
        if let Some(p) = rest.strip_prefix('"')
            && let Some(end) = p.find('"')
            && end > 0
        {
            self.file = Some(Arc::from(&p[..end]));
        }
    }

    fn token(&self, kind: TokenKind, line: usize, col: usize, text: String) -> Token {
        Token {
            kind,
            line,
            col,
            text,
            c_line: self.c_line,
            file: self.file.clone(),
            state: ScanState::Initial,
        }
    }

    /// The end of input, read in scanner state `state`.
    fn eof(&self, state: ScanState) -> Token {
        Token {
            state,
            ..self.token(TokenKind::Eof, self.line, self.col, String::new())
        }
    }

    pub fn next_token(&mut self) -> Result<Token> {
        if let Some(state) = self.skip_whitespace_and_comments() {
            return Ok(self.eof(state));
        }

        let line = self.line;
        let col = self.col;
        let start = self.pos;

        let Some(ch) = self.peek_char() else {
            return Ok(self.eof(ScanState::Initial));
        };

        let single = |lexer: &mut Self, kind: TokenKind| {
            lexer.advance_char();
            kind
        };
        let kind = match ch {
            '@' => single(self, TokenKind::Eof),
            '{' => single(self, TokenKind::LBrace),
            '}' => single(self, TokenKind::RBrace),
            '[' => single(self, TokenKind::LBracket),
            ']' => single(self, TokenKind::RBracket),
            ';' => single(self, TokenKind::Semi),
            ',' => single(self, TokenKind::Comma),
            '=' => single(self, TokenKind::Equals),
            ':' => single(self, TokenKind::Colon),
            '+' => single(self, TokenKind::Plus),
            '-' => match self.peek_next_char() {
                Some('>') => {
                    self.advance_char();
                    self.advance_char();
                    TokenKind::Arrow
                }
                Some('-') => {
                    self.advance_char();
                    self.advance_char();
                    TokenKind::DashDash
                }
                _ => self.lex_number(),
            },
            '"' => return Ok(self.lex_quoted_string(line, col)),
            '<' => return Ok(self.lex_html_string(line, col)),
            '.' => self.lex_number(),
            c if c.is_ascii_digit() => self.lex_number(),
            c if is_ident_start(c) => self.lex_ident_or_keyword(),
            other => single(self, TokenKind::Other(other)),
        };
        let text = self.input[start..self.pos].to_string();
        Ok(self.token(kind, line, col, text))
    }

    /// scan.l `{NUMBER}` with `chkNum`'s split: `-?(D+(.D*)?|.D+)`. A `-` or `.` that does
    /// not start a number is a token of its own.
    fn lex_number(&mut self) -> TokenKind {
        let start = self.pos;
        let start_col = self.col;
        if self.peek_char() == Some('-') {
            self.advance_char();
        }

        let mut has_digits = false;
        while self.peek_char().is_some_and(|c| c.is_ascii_digit()) {
            self.advance_char();
            has_digits = true;
        }

        if self.peek_char() == Some('.') {
            self.advance_char();
            while self.peek_char().is_some_and(|c| c.is_ascii_digit()) {
                self.advance_char();
                has_digits = true;
            }
        }

        if !has_digits {
            // Only `-` and `.` were read (no newline): back up to the first character.
            self.pos = start;
            self.col = start_col;
            let ch = self.advance_char().expect("a '-' or '.' was read");
            return TokenKind::Other(ch);
        }

        TokenKind::Ident(self.input[start..self.pos].to_string())
    }

    fn lex_ident_or_keyword(&mut self) -> TokenKind {
        let start = self.pos;
        self.advance_char();
        while self.peek_char().is_some_and(is_ident_continue) {
            self.advance_char();
        }
        let text = &self.input[start..self.pos];
        match text.to_ascii_lowercase().as_str() {
            "strict" => TokenKind::Strict,
            "graph" => TokenKind::Graph,
            "digraph" => TokenKind::Digraph,
            "subgraph" => TokenKind::Subgraph,
            "node" => TokenKind::Node,
            "edge" => TokenKind::Edge,
            _ => TokenKind::Ident(text.to_string()),
        }
    }

    fn lex_quoted_string(&mut self, line: usize, col: usize) -> Token {
        // Consume opening `"`
        self.advance_char();
        let mut buf = String::new();

        while let Some(ch) = self.advance_char() {
            match ch {
                '"' => {
                    return self.token(TokenKind::Quoted(buf), line, col, "\"".to_string());
                }
                '\\' => match self.peek_char() {
                    Some('"') => {
                        self.advance_char();
                        buf.push('"');
                    }
                    Some('\\') => {
                        self.advance_char();
                        buf.push_str("\\\\");
                    }
                    Some('\n') => {
                        // Escaped newline is ignored (line continuation in scan.l)
                        self.advance_char();
                    }
                    Some('\r') => {
                        self.advance_char();
                        if self.peek_char() == Some('\n') {
                            self.advance_char();
                        }
                    }
                    Some(other) => {
                        // Preserve Graphviz escString sequences like \n, \l, \r, \N, \G, etc.
                        self.advance_char();
                        buf.push('\\');
                        buf.push(other);
                    }
                    None => {
                        buf.push('\\');
                    }
                },
                other => buf.push(other),
            }
        }

        self.eof(ScanState::QString(buf))
    }

    fn lex_html_string(&mut self, line: usize, col: usize) -> Token {
        // Consume opening `<`
        self.advance_char();
        let mut depth: usize = 1;
        let mut buf = String::from("<");

        while let Some(ch) = self.advance_char() {
            buf.push(ch);
            if ch == '<' {
                depth += 1;
            } else if ch == '>' {
                depth -= 1;
                if depth == 0 {
                    return self.token(TokenKind::Html(buf), line, col, ">".to_string());
                }
            }
        }

        buf.remove(0);
        self.eof(ScanState::HString(buf))
    }
}

fn is_ident_start(ch: char) -> bool {
    ch.is_ascii_alphabetic() || ch == '_' || (!ch.is_ascii() && ch != '\u{FEFF}')
}

fn is_ident_continue(ch: char) -> bool {
    ch.is_ascii_alphanumeric() || ch == '_' || (!ch.is_ascii() && ch != '\u{FEFF}')
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_keywords_and_identifiers() {
        let mut lexer = Lexer::new("STRICT DiGrApH SubGraph NODE Edge my_id_123 -3.14 .5");
        let tokens = lexer.tokenize_all().unwrap();
        assert_eq!(tokens[0].kind, TokenKind::Strict);
        assert_eq!(tokens[1].kind, TokenKind::Digraph);
        assert_eq!(tokens[2].kind, TokenKind::Subgraph);
        assert_eq!(tokens[3].kind, TokenKind::Node);
        assert_eq!(tokens[4].kind, TokenKind::Edge);
        assert_eq!(tokens[5].kind, TokenKind::Ident("my_id_123".to_string()));
        assert_eq!(tokens[6].kind, TokenKind::Ident("-3.14".to_string()));
        assert_eq!(tokens[7].kind, TokenKind::Ident(".5".to_string()));
        assert_eq!(tokens[8].kind, TokenKind::Eof);
    }

    #[test]
    fn test_html_string_nested_tags() {
        let src = "label=<<TABLE><TR><TD>hello<BR/>world</TD></TR></TABLE>>";
        let mut lexer = Lexer::new(src);
        let tokens = lexer.tokenize_all().unwrap();
        assert_eq!(tokens[0].kind, TokenKind::Ident("label".to_string()));
        assert_eq!(tokens[1].kind, TokenKind::Equals);
        assert_eq!(
            tokens[2].kind,
            TokenKind::Html("<<TABLE><TR><TD>hello<BR/>world</TD></TR></TABLE>>".to_string())
        );
    }
}
