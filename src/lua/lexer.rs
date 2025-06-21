//! Lua lexer for tokenizing source code
//! 
//! This lexer converts Lua source code into a stream of tokens
//! that can be consumed by the parser.

use super::error::{LuaError, Result};
use std::fmt;

/// A token in the Lua source code
#[derive(Debug, Clone, PartialEq)]
pub enum Token {
    // Literals
    Number(f64),
    String(Vec<u8>),
    
    // Identifiers and keywords
    Identifier(String),
    
    // Keywords
    And,
    Break,
    Do,
    Else,
    Elseif,
    End,
    False,
    For,
    Function,
    If,
    In,
    Local,
    Nil,
    Not,
    Or,
    Repeat,
    Return,
    Then,
    True,
    Until,
    While,
    
    // Operators
    Plus,           // +
    Minus,          // -
    Star,           // *
    Slash,          // /
    Percent,        // %
    Caret,          // ^
    Hash,           // #
    Equal,          // ==
    NotEqual,       // ~=
    Less,           // <
    Greater,        // >
    LessEqual,      // <=
    GreaterEqual,   // >=
    Assign,         // =
    Concat,         // ..
    Dots,           // ...
    
    // Punctuation
    LeftParen,      // (
    RightParen,     // )
    LeftBrace,      // {
    RightBrace,     // }
    LeftBracket,    // [
    RightBracket,   // ]
    Semicolon,      // ;
    Colon,          // :
    Comma,          // ,
    Dot,            // .
    
    // End of file
    Eof,
}

impl Token {
    /// Check if this token is a keyword
    fn from_keyword(s: &str) -> Option<Token> {
        match s {
            "and" => Some(Token::And),
            "break" => Some(Token::Break),
            "do" => Some(Token::Do),
            "else" => Some(Token::Else),
            "elseif" => Some(Token::Elseif),
            "end" => Some(Token::End),
            "false" => Some(Token::False),
            "for" => Some(Token::For),
            "function" => Some(Token::Function),
            "if" => Some(Token::If),
            "in" => Some(Token::In),
            "local" => Some(Token::Local),
            "nil" => Some(Token::Nil),
            "not" => Some(Token::Not),
            "or" => Some(Token::Or),
            "repeat" => Some(Token::Repeat),
            "return" => Some(Token::Return),
            "then" => Some(Token::Then),
            "true" => Some(Token::True),
            "until" => Some(Token::Until),
            "while" => Some(Token::While),
            _ => None,
        }
    }
}

impl fmt::Display for Token {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Token::Number(n) => write!(f, "{}", n),
            Token::String(s) => write!(f, "{:?}", String::from_utf8_lossy(s)),
            Token::Identifier(s) => write!(f, "{}", s),
            Token::Eof => write!(f, "EOF"),
            _ => write!(f, "{:?}", self),
        }
    }
}

/// Lexer for Lua source code
pub struct Lexer<'a> {
    /// Source code being lexed
    input: &'a [u8],
    
    /// Current position in the input
    position: usize,
    
    /// Current line number (1-based)
    line: usize,
    
    /// Current column (1-based)
    column: usize,
}

impl<'a> Lexer<'a> {
    /// Create a new lexer for the given input
    pub fn new(input: &'a str) -> Self {
        Lexer {
            input: input.as_bytes(),
            position: 0,
            line: 1,
            column: 1,
        }
    }
    
    /// Get the next token
    pub fn next_token(&mut self) -> Result<Token> {
        self.skip_whitespace_and_comments();
        
        if self.is_at_end() {
            return Ok(Token::Eof);
        }
        
        let ch = self.current_char();
        
        match ch {
            b'+' => self.single_char_token(Token::Plus),
            b'-' => {
                if self.peek_char() == Some(b'-') {
                    // Comment, skip it
                    self.skip_comment();
                    self.next_token()
                } else {
                    self.single_char_token(Token::Minus)
                }
            }
            b'*' => self.single_char_token(Token::Star),
            b'/' => self.single_char_token(Token::Slash),
            b'%' => self.single_char_token(Token::Percent),
            b'^' => self.single_char_token(Token::Caret),
            b'#' => self.single_char_token(Token::Hash),
            b'(' => self.single_char_token(Token::LeftParen),
            b')' => self.single_char_token(Token::RightParen),
            b'{' => self.single_char_token(Token::LeftBrace),
            b'}' => self.single_char_token(Token::RightBrace),
            b'[' => {
                if self.peek_char() == Some(b'[') || self.peek_char() == Some(b'=') {
                    self.read_long_string()
                } else {
                    self.single_char_token(Token::LeftBracket)
                }
            }
            b']' => self.single_char_token(Token::RightBracket),
            b';' => self.single_char_token(Token::Semicolon),
            b':' => self.single_char_token(Token::Colon),
            b',' => self.single_char_token(Token::Comma),
            b'=' => {
                self.advance();
                if self.current_char() == b'=' {
                    self.advance();
                    Ok(Token::Equal)
                } else {
                    Ok(Token::Assign)
                }
            }
            b'<' => {
                self.advance();
                if self.current_char() == b'=' {
                    self.advance();
                    Ok(Token::LessEqual)
                } else {
                    Ok(Token::Less)
                }
            }
            b'>' => {
                self.advance();
                if self.current_char() == b'=' {
                    self.advance();
                    Ok(Token::GreaterEqual)
                } else {
                    Ok(Token::Greater)
                }
            }
            b'~' => {
                self.advance();
                if self.current_char() == b'=' {
                    self.advance();
                    Ok(Token::NotEqual)
                } else {
                    Err(LuaError::Syntax(format!("unexpected character '~' at line {}", self.line)))
                }
            }
            b'.' => {
                self.advance();
                if self.current_char() == b'.' {
                    self.advance();
                    if self.current_char() == b'.' {
                        self.advance();
                        Ok(Token::Dots)
                    } else {
                        Ok(Token::Concat)
                    }
                } else if self.current_char().is_ascii_digit() {
                    // Number starting with decimal point
                    self.position -= 1;
                    self.read_number()
                } else {
                    Ok(Token::Dot)
                }
            }
            b'"' | b'\'' => self.read_string(ch),
            b'0'..=b'9' => self.read_number(),
            b'a'..=b'z' | b'A'..=b'Z' | b'_' => self.read_identifier(),
            _ => Err(LuaError::Syntax(format!("unexpected character '{}' at line {}", ch as char, self.line))),
        }
    }
    
    /// Get current character
    fn current_char(&self) -> u8 {
        self.input[self.position]
    }
    
    /// Peek at the next character without consuming it
    fn peek_char(&self) -> Option<u8> {
        if self.position + 1 < self.input.len() {
            Some(self.input[self.position + 1])
        } else {
            None
        }
    }
    
    /// Advance to the next character
    fn advance(&mut self) {
        if self.position < self.input.len() {
            if self.input[self.position] == b'\n' {
                self.line += 1;
                self.column = 1;
            } else {
                self.column += 1;
            }
            self.position += 1;
        }
    }
    
    /// Get the current line number
    pub fn line(&self) -> usize {
        self.line
    }
    
    /// Get the current column number
    pub fn column(&self) -> usize {
        self.column
    }
    
    /// Check if we're at the end of input
    fn is_at_end(&self) -> bool {
        self.position >= self.input.len()
    }
    
    /// Create a single-character token and advance
    fn single_char_token(&mut self, token: Token) -> Result<Token> {
        self.advance();
        Ok(token)
    }
    
    /// Skip whitespace and comments
    fn skip_whitespace_and_comments(&mut self) {
        while !self.is_at_end() {
            match self.current_char() {
                b' ' | b'\t' | b'\r' | b'\n' => self.advance(),
                b'-' if self.peek_char() == Some(b'-') => self.skip_comment(),
                _ => break,
            }
        }
    }
    
    /// Skip a comment
    fn skip_comment(&mut self) {
        // Skip --
        self.advance();
        self.advance();
        
        // Check for long comment
        if !self.is_at_end() && self.current_char() == b'[' {
            let next = self.peek_char();
            if next == Some(b'[') || next == Some(b'=') {
                // Long comment, skip until ]]
                self.skip_long_comment();
                return;
            }
        }
        
        // Short comment, skip until end of line
        while !self.is_at_end() && self.current_char() != b'\n' {
            self.advance();
        }
    }
    
    /// Skip a long comment
    fn skip_long_comment(&mut self) {
        // Count the number of = signs
        self.advance(); // skip [
        let mut level = 0;
        while !self.is_at_end() && self.current_char() == b'=' {
            level += 1;
            self.advance();
        }
        
        if self.is_at_end() || self.current_char() != b'[' {
            return; // Invalid long comment start
        }
        self.advance(); // skip second [
        
        // Find matching closing bracket
        while !self.is_at_end() {
            if self.current_char() == b']' {
                self.advance();
                let mut close_level = 0;
                while !self.is_at_end() && self.current_char() == b'=' {
                    close_level += 1;
                    self.advance();
                }
                if !self.is_at_end() && self.current_char() == b']' && close_level == level {
                    self.advance();
                    break;
                }
            } else {
                self.advance();
            }
        }
    }
    
    /// Read a number token
    fn read_number(&mut self) -> Result<Token> {
        let start = self.position;
        
        // Integer part
        while !self.is_at_end() && self.current_char().is_ascii_digit() {
            self.advance();
        }
        
        // Decimal part
        if !self.is_at_end() && self.current_char() == b'.' && 
           self.peek_char().map_or(false, |c| c.is_ascii_digit()) {
            self.advance(); // skip .
            while !self.is_at_end() && self.current_char().is_ascii_digit() {
                self.advance();
            }
        }
        
        // Exponent part
        if !self.is_at_end() && (self.current_char() == b'e' || self.current_char() == b'E') {
            self.advance();
            if !self.is_at_end() && (self.current_char() == b'+' || self.current_char() == b'-') {
                self.advance();
            }
            while !self.is_at_end() && self.current_char().is_ascii_digit() {
                self.advance();
            }
        }
        
        let num_str = std::str::from_utf8(&self.input[start..self.position])
            .map_err(|_| LuaError::Syntax("invalid UTF-8 in number".to_string()))?;
        
        let num = num_str.parse::<f64>()
            .map_err(|_| LuaError::Syntax(format!("invalid number: {}", num_str)))?;
        
        Ok(Token::Number(num))
    }
    
    /// Read a string token
    fn read_string(&mut self, quote: u8) -> Result<Token> {
        self.advance(); // skip opening quote
        let mut value = Vec::new();
        
        while !self.is_at_end() && self.current_char() != quote {
            if self.current_char() == b'\\' {
                self.advance();
                if self.is_at_end() {
                    return Err(LuaError::Syntax("unterminated string".to_string()));
                }
                
                let escaped = match self.current_char() {
                    b'a' => b'\x07',  // bell
                    b'b' => b'\x08',  // backspace
                    b'f' => b'\x0C',  // form feed
                    b'n' => b'\n',    // newline
                    b'r' => b'\r',    // carriage return
                    b't' => b'\t',    // tab
                    b'v' => b'\x0B',  // vertical tab
                    b'\\' => b'\\',
                    b'"' => b'"',
                    b'\'' => b'\'',
                    b'\n' => b'\n',   // literal newline in string
                    _ => self.current_char(), // unknown escape, keep as-is
                };
                value.push(escaped);
                self.advance();
            } else {
                value.push(self.current_char());
                self.advance();
            }
        }
        
        if self.is_at_end() {
            return Err(LuaError::Syntax("unterminated string".to_string()));
        }
        
        self.advance(); // skip closing quote
        Ok(Token::String(value))
    }
    
    /// Read a long string (long bracket notation)
    fn read_long_string(&mut self) -> Result<Token> {
        // Count the number of = signs
        self.advance(); // skip [
        let mut level = 0;
        while !self.is_at_end() && self.current_char() == b'=' {
            level += 1;
            self.advance();
        }
        
        if self.is_at_end() || self.current_char() != b'[' {
            // Not a long string, backup
            self.position -= level + 1;
            return self.single_char_token(Token::LeftBracket);
        }
        self.advance(); // skip second [
        
        let mut value = Vec::new();
        
        // Find matching closing bracket
        while !self.is_at_end() {
            if self.current_char() == b']' {
                let save_pos = self.position;
                self.advance();
                let mut close_level = 0;
                while !self.is_at_end() && self.current_char() == b'=' {
                    close_level += 1;
                    self.advance();
                }
                if !self.is_at_end() && self.current_char() == b']' && close_level == level {
                    self.advance();
                    break;
                } else {
                    // False alarm, restore position and add the ] to value
                    for i in save_pos..self.position {
                        value.push(self.input[i]);
                    }
                }
            } else {
                value.push(self.current_char());
                self.advance();
            }
        }
        
        Ok(Token::String(value))
    }
    
    /// Read an identifier or keyword
    fn read_identifier(&mut self) -> Result<Token> {
        let start = self.position;
        
        while !self.is_at_end() && 
              (self.current_char().is_ascii_alphanumeric() || self.current_char() == b'_') {
            self.advance();
        }
        
        let ident = std::str::from_utf8(&self.input[start..self.position])
            .map_err(|_| LuaError::Syntax("invalid UTF-8 in identifier".to_string()))?;
        
        Ok(Token::from_keyword(ident).unwrap_or_else(|| Token::Identifier(ident.to_string())))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_simple_tokens() {
        let mut lexer = Lexer::new("+ - * / % ^ #");
        assert_eq!(lexer.next_token().unwrap(), Token::Plus);
        assert_eq!(lexer.next_token().unwrap(), Token::Minus);
        assert_eq!(lexer.next_token().unwrap(), Token::Star);
        assert_eq!(lexer.next_token().unwrap(), Token::Slash);
        assert_eq!(lexer.next_token().unwrap(), Token::Percent);
        assert_eq!(lexer.next_token().unwrap(), Token::Caret);
        assert_eq!(lexer.next_token().unwrap(), Token::Hash);
        assert_eq!(lexer.next_token().unwrap(), Token::Eof);
    }
    
    #[test]
    fn test_numbers() {
        let mut lexer = Lexer::new("42 3.14 1e10 0.5e-3");
        assert_eq!(lexer.next_token().unwrap(), Token::Number(42.0));
        assert_eq!(lexer.next_token().unwrap(), Token::Number(3.14));
        assert_eq!(lexer.next_token().unwrap(), Token::Number(1e10));
        assert_eq!(lexer.next_token().unwrap(), Token::Number(0.5e-3));
    }
    
    #[test]
    fn test_strings() {
        let mut lexer = Lexer::new(r#""hello" 'world' "with\nescapes""#);
        assert_eq!(lexer.next_token().unwrap(), Token::String(b"hello".to_vec()));
        assert_eq!(lexer.next_token().unwrap(), Token::String(b"world".to_vec()));
        assert_eq!(lexer.next_token().unwrap(), Token::String(b"with\nescapes".to_vec()));
    }
    
    #[test]
    fn test_keywords_and_identifiers() {
        let mut lexer = Lexer::new("if then else foo bar_baz");
        assert_eq!(lexer.next_token().unwrap(), Token::If);
        assert_eq!(lexer.next_token().unwrap(), Token::Then);
        assert_eq!(lexer.next_token().unwrap(), Token::Else);
        assert_eq!(lexer.next_token().unwrap(), Token::Identifier("foo".to_string()));
        assert_eq!(lexer.next_token().unwrap(), Token::Identifier("bar_baz".to_string()));
    }
    
    #[test]
    fn test_comments() {
        let mut lexer = Lexer::new("42 -- comment\n43");
        assert_eq!(lexer.next_token().unwrap(), Token::Number(42.0));
        assert_eq!(lexer.next_token().unwrap(), Token::Number(43.0));
    }
}