//! Re-entrant DOT lexer matching Graphviz `lib/cgraph/scan.l`.

use gv_core::{GraphvizError, Result};

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
    Eof,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Token {
    pub kind: TokenKind,
    pub line: usize,
    pub col: usize,
}

pub struct Lexer<'a> {
    input: &'a str,
    pos: usize,
    line: usize,
    col: usize,
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
            self.col = 1;
        } else {
            self.col += 1;
        }
        Some(ch)
    }

    fn skip_whitespace_and_comments(&mut self) -> Result<()> {
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
                    self.advance_char();
                    self.advance_char();
                    while let Some(ch) = self.peek_char() {
                        self.advance_char();
                        if ch == '\n' {
                            break;
                        }
                    }
                }
                // C-style block comment `/* ... */`
                (Some('/'), Some('*')) => {
                    let start_line = self.line;
                    let start_col = self.col;
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
                        return Err(GraphvizError::SyntaxError {
                            line: start_line,
                            col: start_col,
                            message: "unclosed /* ... */ block comment".to_string(),
                        });
                    }
                }
                // Preprocessor / shell-like comment `# ...`
                (Some('#'), _) => {
                    while let Some(ch) = self.peek_char() {
                        self.advance_char();
                        if ch == '\n' {
                            break;
                        }
                    }
                }
                _ => break,
            }
        }
        Ok(())
    }

    pub fn next_token(&mut self) -> Result<Token> {
        self.skip_whitespace_and_comments()?;

        let line = self.line;
        let col = self.col;

        let Some(ch) = self.peek_char() else {
            return Ok(Token {
                kind: TokenKind::Eof,
                line,
                col,
            });
        };

        // Punctuation & operators
        match ch {
            '{' => {
                self.advance_char();
                Ok(Token {
                    kind: TokenKind::LBrace,
                    line,
                    col,
                })
            }
            '}' => {
                self.advance_char();
                Ok(Token {
                    kind: TokenKind::RBrace,
                    line,
                    col,
                })
            }
            '[' => {
                self.advance_char();
                Ok(Token {
                    kind: TokenKind::LBracket,
                    line,
                    col,
                })
            }
            ']' => {
                self.advance_char();
                Ok(Token {
                    kind: TokenKind::RBracket,
                    line,
                    col,
                })
            }
            ';' => {
                self.advance_char();
                Ok(Token {
                    kind: TokenKind::Semi,
                    line,
                    col,
                })
            }
            ',' => {
                self.advance_char();
                Ok(Token {
                    kind: TokenKind::Comma,
                    line,
                    col,
                })
            }
            '=' => {
                self.advance_char();
                Ok(Token {
                    kind: TokenKind::Equals,
                    line,
                    col,
                })
            }
            ':' => {
                self.advance_char();
                Ok(Token {
                    kind: TokenKind::Colon,
                    line,
                    col,
                })
            }
            '+' => {
                self.advance_char();
                Ok(Token {
                    kind: TokenKind::Plus,
                    line,
                    col,
                })
            }
            '-' => match self.peek_next_char() {
                Some('>') => {
                    self.advance_char();
                    self.advance_char();
                    Ok(Token {
                        kind: TokenKind::Arrow,
                        line,
                        col,
                    })
                }
                Some('-') => {
                    self.advance_char();
                    self.advance_char();
                    Ok(Token {
                        kind: TokenKind::DashDash,
                        line,
                        col,
                    })
                }
                Some(next) if next.is_ascii_digit() || next == '.' => self.lex_number(line, col),
                _ => {
                    self.advance_char();
                    Err(GraphvizError::SyntaxError {
                        line,
                        col,
                        message: "unexpected '-'".to_string(),
                    })
                }
            },
            '"' => self.lex_quoted_string(line, col),
            '<' => self.lex_html_string(line, col),
            '.' => {
                if self.peek_next_char().is_some_and(|c| c.is_ascii_digit()) {
                    return self.lex_number(line, col);
                }
                self.advance_char();
                Err(GraphvizError::SyntaxError {
                    line,
                    col,
                    message: "unexpected '.'".to_string(),
                })
            }
            c if c.is_ascii_digit() => self.lex_number(line, col),
            c if is_ident_start(c) => Ok(self.lex_ident_or_keyword(line, col)),
            other => {
                self.advance_char();
                Err(GraphvizError::SyntaxError {
                    line,
                    col,
                    message: format!("unexpected character '{other}'"),
                })
            }
        }
    }

    fn lex_number(&mut self, line: usize, col: usize) -> Result<Token> {
        let start = self.pos;
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
            return Err(GraphvizError::SyntaxError {
                line,
                col,
                message: "invalid numeric literal".to_string(),
            });
        }

        let text = &self.input[start..self.pos];
        Ok(Token {
            kind: TokenKind::Ident(text.to_string()),
            line,
            col,
        })
    }

    fn lex_ident_or_keyword(&mut self, line: usize, col: usize) -> Token {
        let start = self.pos;
        self.advance_char();
        while self.peek_char().is_some_and(is_ident_continue) {
            self.advance_char();
        }
        let text = &self.input[start..self.pos];
        let kind = match text.to_ascii_lowercase().as_str() {
            "strict" => TokenKind::Strict,
            "graph" => TokenKind::Graph,
            "digraph" => TokenKind::Digraph,
            "subgraph" => TokenKind::Subgraph,
            "node" => TokenKind::Node,
            "edge" => TokenKind::Edge,
            _ => TokenKind::Ident(text.to_string()),
        };
        Token { kind, line, col }
    }

    fn lex_quoted_string(&mut self, line: usize, col: usize) -> Result<Token> {
        // Consume opening `"`
        self.advance_char();
        let mut buf = String::new();

        while let Some(ch) = self.advance_char() {
            match ch {
                '"' => {
                    return Ok(Token {
                        kind: TokenKind::Quoted(buf),
                        line,
                        col,
                    });
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

        Err(GraphvizError::SyntaxError {
            line,
            col,
            message: "unclosed quoted string literal".to_string(),
        })
    }

    fn lex_html_string(&mut self, line: usize, col: usize) -> Result<Token> {
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
                    return Ok(Token {
                        kind: TokenKind::Html(buf),
                        line,
                        col,
                    });
                }
            }
        }

        Err(GraphvizError::SyntaxError {
            line,
            col,
            message: "unclosed HTML-like string '<...>'".to_string(),
        })
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
