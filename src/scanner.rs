use crate::token::{keyword, Token, TokenType};

/// Cut `source` into tokens. Returns everything it managed to scan alongside every
/// error it found; the caller decides whether to go on.
pub fn scan(source: &str) -> (Vec<Token>, Vec<String>) {
    let mut s = Scanner {
        src: source.chars().collect(),
        start: 0,
        current: 0,
        line: 1,
        tokens: Vec::new(),
        errors: Vec::new(),
    };
    s.run();
    (s.tokens, s.errors)
}

struct Scanner {
    src: Vec<char>,
    start: usize,
    current: usize,
    line: usize,
    tokens: Vec<Token>,
    errors: Vec<String>,
}

impl Scanner {
    fn run(&mut self) {
        while !self.at_end() {
        self.start = self.current;
        self.scan_token();
        }

        let eof_line = self.tokens.last()
            .map(|token| token.line)
            .unwrap_or(1);

        self.tokens.push(Token {
            kind: TokenType::Eof,
            lexeme: String::new(),
            line: eof_line,
        });
        
    }

    fn scan_token(&mut self) {
        let c = self.advance();

        match c {
            // Single-character tokens
            '(' => self.add(TokenType::LParen),
            ')' => self.add(TokenType::RParen),
            '{' => self.add(TokenType::LBrace),
            '}' => self.add(TokenType::RBrace),
            ',' => self.add(TokenType::Comma),
            ';' => self.add(TokenType::Semicolon),
            '+' => self.add(TokenType::Plus),
            '-' => self.add(TokenType::Minus),
            '*' => self.add(TokenType::Star),

            // One- or two-character tokens
            '!' => {
                let t = if self.matches('=') {
                    TokenType::BangEqual
                } else {
                    TokenType::Bang
                };
                self.add(t);
            }

            // Whitespace
            ' ' | '\r' | '\t' => {}

            // Newline
            '\n' => self.line += 1,

            // Slash or comment
            '/' => {
                if self.matches('/') {
                    while !self.at_end() && self.peek() != '\n' {
                        self.advance();
                    }
                } else {
                    self.add(TokenType::Slash);
                }
            }

            c if c.is_ascii_digit() => self.number(),
            c if c.is_ascii_alphabetic() || c == '_' => self.identifier(),
            '"' => self.string(),
            _ => self.error(self.line, "Character is not part of any token."),

        }
    }

    fn string(&mut self) {
        let opening_line = self.line;

        while self.peek() != '"' && !self.at_end() {
            if self.peek() == '\n' {
                self.line += 1;
            }

            self.advance();
        }

        if self.at_end() {
            self.error(opening_line, "String is never closed.");
            return;
        }

        self.advance(); // consume closing '"'
        self.add(TokenType::Str);
    }

    fn number(&mut self) {
        while self.peek().is_ascii_digit() {
            self.advance();
        }

        // Fractional part: only consume '.' if followed by a digit
        if self.peek() == '.' && self.peek_next().is_ascii_digit() {
            self.advance();

            while self.peek().is_ascii_digit() {
                self.advance();
            }
        }

        self.add(TokenType::Number);
    }

    fn identifier(&mut self) {
        while self.peek().is_ascii_alphanumeric() || self.peek() == '_' {
            self.advance();
        }

        // Read the complete lexeme before looking it up as a keyword.
        let text: String = self.src[self.start..self.current]
            .iter()
            .collect();

        match keyword(&text) {
            Some(kind) => self.add(kind),
            None => self.add(TokenType::Identifier),
        }
    }


    // --- primitives ---------------------------------------------------------------

    fn at_end(&self) -> bool {
        self.current >= self.src.len()
    }

    fn advance(&mut self) -> char {
        let c = self.src[self.current];
        self.current += 1;
        c
    }

    fn matches(&mut self, expected: char) -> bool {
        if self.at_end() || self.src[self.current] != expected {
            return false;
        }
        self.current += 1;
        true
    }

    fn peek(&self) -> char {
        if self.at_end() {
            '\0'
        } else {
            self.src[self.current]
        }
    }

    fn peek_next(&self) -> char {
        if self.current + 1 >= self.src.len() {
            '\0'
        } else {
            self.src[self.current + 1]
        }
    }

    fn add(&mut self, kind: TokenType) {
        self.tokens.push(Token {
            kind,
            lexeme: self.src[self.start..self.current].iter().collect(),
            line: self.line,
        });
    }

    fn error(&mut self, line: usize, message: &str) {
        self.errors.push(format!("[line {}] Error: {}", line, message));
    }
}
