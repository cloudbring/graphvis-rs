//! Re-entrant DOT lexer matching Graphviz `lib/cgraph/scan.l`.
//!
//! Like the flex scanner it never fails. A character no rule matches is a token of its own
//! (scan.l's `.` rule returns `yytext[0]`), which the grammar then rejects, and end of input
//! inside a quoted string, an HTML string or a comment ends the token stream with the scanner
//! state kept for `aagerror`'s message.
//!
//! The rules are scan.l's, tried in its order with flex's longest match, and the input is
//! what cgraph's `fgets` reader passes on ([`fgets_view`]).

use std::borrow::Cow;
use std::cell::RefCell;
use std::sync::Arc;

use graphviz_pure_core::{Result, charset, diag};

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

thread_local! {
    static INPUT_NAME: RefCell<Option<Arc<str>>> = const { RefCell::new(None) };
}

/// Runs `f` with `name` as the name of the input being read (`agconcat`'s `filename`, which
/// becomes the scanner's `InputFile`). `chkNum` names it in its warning, or `input` when
/// there is none, as for a library call (`agmemread`).
pub fn with_input_name<R>(name: &str, f: impl FnOnce() -> R) -> R {
    struct Restore(Option<Arc<str>>);
    impl Drop for Restore {
        fn drop(&mut self) {
            let prev = self.0.take();
            INPUT_NAME.with(|n| *n.borrow_mut() = prev);
        }
    }
    let _restore = Restore(INPUT_NAME.with(|n| n.replace(Some(Arc::from(name)))));
    f()
}

/// The text cgraph's scanner reads from `input`. `ioread` (`lib/cgraph/io.c`) reads a line at
/// a time with `fgets` and returns `strlen` of it, so a NUL hides the rest of its line,
/// newline included, and a line that starts with a NUL reads as the end of the input.
/// Returns that text and the offsets in it where a hidden newline was.
fn fgets_view(input: &str) -> (Cow<'_, str>, Vec<usize>) {
    if !input.contains('\0') {
        return (Cow::Borrowed(input), Vec::new());
    }
    let mut text = String::with_capacity(input.len());
    let mut hidden_eols = Vec::new();
    for line in input.split_inclusive('\n') {
        match line.find('\0') {
            None => text.push_str(line),
            Some(0) => break,
            Some(nul) => {
                text.push_str(&line[..nul]);
                if line.ends_with('\n') {
                    hidden_eols.push(text.len());
                }
            }
        }
    }
    (Cow::Owned(text), hidden_eols)
}

pub struct Lexer<'a> {
    /// The input as the scanner reads it ([`fgets_view`]).
    input: Cow<'a, str>,
    pos: usize,
    /// Line of the input (1-based), counting the newlines a NUL hid.
    line: usize,
    col: usize,
    /// cgraph's `line_num`.
    c_line: i32,
    /// cgraph's `InputFile`, when a line directive named one.
    file: Option<Arc<str>>,
    /// `agconcat`'s `filename` ([`with_input_name`]).
    input_name: Option<Arc<str>>,
    /// Offsets in `input` where a newline hidden by a NUL was, and the next one to pass.
    hidden_eols: Vec<usize>,
    next_hidden_eol: usize,
}

impl<'a> Lexer<'a> {
    pub fn new(input: &'a str) -> Self {
        let (input, hidden_eols) = fgets_view(input);
        Self {
            input,
            pos: 0,
            line: 1,
            col: 1,
            c_line: 1,
            file: None,
            input_name: INPUT_NAME.with(|n| n.borrow().clone()),
            hidden_eols,
            next_hidden_eol: 0,
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
        if self.hidden_eols.get(self.next_hidden_eol) == Some(&self.pos) {
            self.next_hidden_eol += 1;
            self.line += 1;
            self.col = 1;
        }
        Some(ch)
    }

    /// Consumes the rest of the line, not its newline (flex's `.*`).
    fn skip_to_eol(&mut self) {
        while self.peek_char().is_some_and(|ch| ch != '\n') {
            self.advance_char();
        }
    }

    /// The rules of scan.l that return no token, in scan.l's order: `\n`, comments, line
    /// directives, white space and the byte-order mark. Returns [`ScanState::Comment`] when
    /// the input ends inside a `/* ... */` comment.
    fn skip_whitespace_and_comments(&mut self) -> Option<ScanState> {
        loop {
            match (self.peek_char(), self.peek_next_char()) {
                (Some(' ' | '\t' | '\r' | '\n'), _) => {
                    self.advance_char();
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
                // C++-style line comment `// ...`
                (Some('/'), Some('/')) => self.skip_to_eol(),
                // `^"#".*` is a line directive (`ppDirective`), `"#".*` a comment.
                (Some('#'), _) => {
                    let at_bol = self.pos == 0 || self.input.as_bytes()[self.pos - 1] == b'\n';
                    let start = self.pos;
                    self.skip_to_eol();
                    if at_bol {
                        let text = self.input[start + 1..self.pos].to_string();
                        self.pp_directive(&text);
                    }
                }
                // `"\xEF\xBB\xBF"` (ignore BOM). `LETTER` holds every byte from \200, so a
                // BOM followed by a letter or digit is the longer `{NAME}` match; alone it
                // ties with a 3-byte `{NAME}` and the earlier rule wins.
                (Some('\u{FEFF}'), next) if !next.is_some_and(is_name_char) => {
                    self.advance_char();
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
        // `%d` into an `int`: the conversion saturates at 64 bits (to INT64_MIN when
        // negative), the store keeps the low 32
        let value = digits[..n_digits].bytes().fold(0i64, |acc, d| {
            let d = i64::from(d - b'0');
            let acc = acc.saturating_mul(10);
            if neg {
                acc.saturating_sub(d)
            } else {
                acc.saturating_add(d)
            }
        });
        let lineno = value as i32;
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
        Ok(self.scan())
    }

    /// The next token. Like flex's `yylex`, it never fails.
    pub(crate) fn scan(&mut self) -> Token {
        if let Some(state) = self.skip_whitespace_and_comments() {
            return self.eof(state);
        }

        let line = self.line;
        let col = self.col;
        let start = self.pos;

        let Some(ch) = self.peek_char() else {
            return self.eof(ScanState::Initial);
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
            '"' => return self.lex_quoted_string(line, col),
            '<' => return self.lex_html_string(line, col),
            '.' | '0'..='9' => self.lex_number(),
            c if is_letter(c) => self.lex_ident_or_keyword(),
            other => single(self, TokenKind::Other(other)),
        };
        let text = self.input[start..self.pos].to_string();
        self.token(kind, line, col, text)
    }

    /// scan.l `{NUMBER}`, `[-]?(({DIGIT}+(\.{DIGIT}*)?)|(\.{DIGIT}+))(\.|{LETTER})?`, then
    /// `chkNum`: a match that ends in a letter, or in a second `.`, is warned about and
    /// gives its last character back (`yyless`). A `-` or `.` that starts no number is
    /// scan.l's `.` rule.
    fn lex_number(&mut self) -> TokenKind {
        let rest = &self.input[self.pos..];
        let bytes = rest.as_bytes();
        let digits_from = |i: usize| bytes[i..].iter().take_while(|b| b.is_ascii_digit()).count();
        let mut len = usize::from(bytes[0] == b'-');
        let int_digits = digits_from(len);
        len += int_digits;
        let mut frac_digits = 0;
        if bytes.get(len) == Some(&b'.') {
            frac_digits = digits_from(len + 1);
            if int_digits > 0 || frac_digits > 0 {
                len += 1 + frac_digits;
            }
        }
        if int_digits == 0 && frac_digits == 0 {
            let ch = rest.chars().next().unwrap_or('-');
            self.advance_char();
            return TokenKind::Other(ch);
        }
        let number = rest[..len].to_string();
        // The optional `(\.|{LETTER})` ends the match; a `.` there is always a second one
        // (the first `.` after digits is the fraction's), so `chkNum` warns whenever it
        // matched.
        if let Some(c) = rest[len..]
            .chars()
            .next()
            .filter(|&c| c == '.' || is_letter(c))
        {
            // `LETTER` is a byte: of a multibyte character, `yytext` holds the first.
            let last = if c.is_ascii() {
                c
            } else {
                charset::raw_char(charset::encode(c.encode_utf8(&mut [0; 4]))[0])
            };
            let fname = self
                .file
                .as_deref()
                .or(self.input_name.as_deref())
                .unwrap_or("input");
            diag::agwarningf(&format!(
                "syntax ambiguity - badly delimited number '{number}{last}' in line {} of \
                 {fname} splits into two tokens\n",
                self.c_line
            ));
        }
        for _ in 0..len {
            self.advance_char();
        }
        TokenKind::Ident(number)
    }

    /// scan.l's keyword rules and `{NAME}`: a keyword ties with a `{NAME}` of its length and,
    /// being the earlier rule, wins. Keywords match without regard to case.
    fn lex_ident_or_keyword(&mut self) -> TokenKind {
        let start = self.pos;
        self.advance_char();
        while self.peek_char().is_some_and(is_name_char) {
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

    /// scan.l's `<qstring>` rules.
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
                    // `<qstring>[\\]["]`
                    Some('"') => {
                        self.advance_char();
                        buf.push('"');
                    }
                    // `<qstring>[\\][\\]`
                    Some('\\') => {
                        self.advance_char();
                        buf.push_str("\\\\");
                    }
                    // `<qstring>[\\][\n]`: an escaped newline is ignored. Only a newline:
                    // before a CR, the backslash is kept like any other.
                    Some('\n') => {
                        self.advance_char();
                    }
                    // `<qstring>[\\]`: kept, and the next character is read on its own,
                    // which keeps Graphviz escString sequences like \n, \l, \N, \G.
                    _ => buf.push('\\'),
                },
                other => buf.push(other),
            }
        }

        self.eof(ScanState::QString(buf))
    }

    /// scan.l's `<hstring>` rules.
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

/// scan.l `LETTER`, `[A-Za-z_\200-\377]`: every non-ASCII character is made of such bytes
/// (a raw byte of non-UTF-8 input too).
fn is_letter(ch: char) -> bool {
    ch.is_ascii_alphabetic() || ch == '_' || !ch.is_ascii()
}

/// `{LETTER}|{DIGIT}`, the characters that continue a `{NAME}`.
fn is_name_char(ch: char) -> bool {
    is_letter(ch) || ch.is_ascii_digit()
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

    fn kinds(src: &str) -> Vec<TokenKind> {
        Lexer::new(src)
            .tokenize_all()
            .unwrap()
            .into_iter()
            .map(|t| t.kind)
            .collect()
    }

    fn ident(s: &str) -> TokenKind {
        TokenKind::Ident(s.to_string())
    }

    #[test]
    fn number_running_into_a_name_splits_like_chknum() {
        // `{NUMBER}` takes one trailing letter or `.`; `chkNum` gives it back (`yyless`).
        assert_eq!(
            kinds("2x 1.2.3 1. 1.. -.5x 5\u{e9} -.x"),
            [
                ident("2"),
                ident("x"),
                ident("1.2"),
                ident(".3"),
                ident("1."),
                ident("1."),
                TokenKind::Other('.'),
                ident("-.5"),
                ident("x"),
                ident("5"),
                ident("\u{e9}"),
                TokenKind::Other('-'),
                TokenKind::Other('.'),
                ident("x"),
                TokenKind::Eof,
            ]
        );
    }

    #[test]
    fn byte_order_mark_is_skipped_alone_and_kept_in_a_name() {
        let bom = "\u{feff}";
        assert_eq!(
            kinds(&format!("{bom} a {bom} b{bom} {bom}{bom} {bom}5 node{bom}")),
            [
                ident("a"),
                ident(&format!("b{bom}")),
                ident(&format!("{bom}{bom}")),
                ident(&format!("{bom}5")),
                ident(&format!("node{bom}")),
                TokenKind::Eof,
            ]
        );
        assert_eq!(
            kinds(&format!("{bom}digraph")),
            [ident(&format!("{bom}digraph")), TokenKind::Eof]
        );
        assert_eq!(
            kinds(&format!("{bom}{{")),
            [TokenKind::LBrace, TokenKind::Eof]
        );
    }

    #[test]
    fn backslash_is_dropped_only_before_lf() {
        assert_eq!(
            kinds("\"a\\\nb\" \"a\\\r\nb\" \"a\\rb\""),
            [
                TokenKind::Quoted("ab".to_string()),
                TokenKind::Quoted("a\\\r\nb".to_string()),
                TokenKind::Quoted("a\\rb".to_string()),
                TokenKind::Eof,
            ]
        );
    }

    #[test]
    fn nul_hides_the_rest_of_its_line_like_fgets() {
        let tokens = Lexer::new("a \0 b\nc\0\nd\n\0e\nf").tokenize_all().unwrap();
        let got: Vec<_> = tokens
            .iter()
            .map(|t| (t.kind.clone(), t.line, t.c_line))
            .collect();
        // `c` and `d` join: the newline after `c` is hidden. The line that starts with a NUL
        // ends the input. `line` counts the hidden newlines, `c_line` does not.
        assert_eq!(
            got,
            [
                (ident("a"), 1, 1),
                (ident("cd"), 2, 1),
                (TokenKind::Eof, 4, 2),
            ]
        );
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
