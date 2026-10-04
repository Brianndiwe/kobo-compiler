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
            _ => {}
        }
    }

    fn string(&mut self) {
        // TODO(you): scan a string literal. A string may span lines, and an unterminated one is
        //            reported at the line it opened on (1.5; the message is in 5.1).
        todo!("string")
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
        // TODO(you): scan an identifier, then decide whether it is a keyword; keyword() in
        //            token.rs does the lookup (1.2, 1.3).
        todo!("identifier")
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
