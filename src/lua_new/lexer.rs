//! Lexer for Lua 5.1

use crate::lua_new::error::{LuaError, Result};
use std::str::Chars;
use std::iter::Peekable;

/// Token types
#[derive(Debug, Clone, PartialEq)]
pub enum TokenType {
    // Literals
    Nil,
    True,
    False,
    Number(f64),
    String(String),
    Identifier(String),
    
    // Keywords
    And,
    Break,
    Do,
    Else,
    Elseif,
    End,
    For,
    Function,
    If,
    In,
    Local,
    Not,
    Or,
    Repeat,
    Return,
    Then,
    Until,
    While,
    
    // Operators
    Plus,         // +
    Minus,        // -
    Multiply,     // *
    Divide,       // /
    Modulo,       // %
    Power,        // ^
    Length,       // #
    Equal,        // ==
    NotEqual,     // ~=
    Less,         // <
    Greater,      // >
    LessEqual,    // <=
    GreaterEqual, // >=
    Assign,       // =
    Concat,       // ..
    Vararg,       // ...
    
    // Punctuation
    LeftParen,    // (
    RightParen,   // )
    LeftBracket,  // [
    RightBracket, // ]
    LeftBrace,    // {
    RightBrace,   // }
    Semicolon,    // ;
    Colon,        // :
    Comma,        // ,
    Dot,          // .
    
    // End of file
    EOF,
}

/// A token with position information
#[derive(Debug, Clone)]
pub struct Token {
    pub token_type: TokenType,
    pub line: u16,
    pub column: u16,
}

/// The lexer
pub struct Lexer<'a> {
    /// Character iterator
    chars: Peekable<Chars<'a>>,
    
    /// Current position
    line: u16,
    column: u16,
    
    /// Source code (for error reporting)
    source: &'a str,
}

impl<'a> Lexer<'a> {
    /// Create a new lexer
    pub fn new(source: &'a str) -> Self {
        Lexer {
            chars: source.chars().peekable(),
            line: 1,
            column: 1,
            source,
        }
    }
    
    /// Get the next token
    pub fn next_token(&mut self) -> Result<Token> {
        // Skip whitespace and comments
        self.skip_whitespace_and_comments()?;
        
        let line = self.line;
        let column = self.column;
        
        // Check for EOF
        if self.peek_char().is_none() {
            return Ok(Token {
                token_type: TokenType::EOF,
                line,
                column,
            });
        }
        
        // Get the next character
        let ch = self.next_char().unwrap();
        
        let token_type = match ch {
            // Single character tokens
            '(' => TokenType::LeftParen,
            ')' => TokenType::RightParen,
            '[' => {
                // Could be [ or [[ for long string
                if self.peek_char() == Some('[') {
                    self.read_long_string()?
                } else {
                    TokenType::LeftBracket
                }
            }
            ']' => TokenType::RightBracket,
            '{' => TokenType::LeftBrace,
            '}' => TokenType::RightBrace,
            ';' => TokenType::Semicolon,
            ':' => TokenType::Colon,
            ',' => TokenType::Comma,
            '#' => TokenType::Length,
            '+' => TokenType::Plus,
            '-' => {
                // Could be - or --
                if self.peek_char() == Some('-') {
                    self.next_char();
                    self.skip_comment()?;
                    return self.next_token();
                }
                TokenType::Minus
            }
            '*' => TokenType::Multiply,
            '/' => TokenType::Divide,
            '%' => TokenType::Modulo,
            '^' => TokenType::Power,
            '=' => {
                // Could be = or ==
                if self.peek_char() == Some('=') {
                    self.next_char();
                    TokenType::Equal
                } else {
                    TokenType::Assign
                }
            }
            '<' => {
                // Could be < or <=
                if self.peek_char() == Some('=') {
                    self.next_char();
                    TokenType::LessEqual
                } else {
                    TokenType::Less
                }
            }
            '>' => {
                // Could be > or >=
                if self.peek_char() == Some('=') {
                    self.next_char();
                    TokenType::GreaterEqual
                } else {
                    TokenType::Greater
                }
            }
            '~' => {
                // Must be ~=
                if self.next_char() == Some('=') {
                    TokenType::NotEqual
                } else {
                    return Err(LuaError::SyntaxError {
                        message: "Invalid character '~'".to_string(),
                        line: line as usize,
                        column: column as usize,
                    });
                }
            }
            '.' => {
                // Could be ., .., or ...
                if self.peek_char() == Some('.') {
                    self.next_char();
                    if self.peek_char() == Some('.') {
                        self.next_char();
                        TokenType::Vararg
                    } else {
                        TokenType::Concat
                    }
                } else if self.peek_char().map(|c| c.is_ascii_digit()).unwrap_or(false) {
                    // Number starting with .
                    self.read_number_fraction()?
                } else {
                    TokenType::Dot
                }
            }
            '\'' | '"' => self.read_string(ch)?,
            _ if ch.is_ascii_digit() => self.read_number(ch)?,
            _ if ch.is_alphabetic() || ch == '_' => self.read_identifier(ch)?,
            _ => {
                return Err(LuaError::SyntaxError {
                    message: format!("Invalid character '{}'", ch),
                    line: line as usize,
                    column: column as usize,
                });
            }
        };
        
        Ok(Token {
            token_type,
            line,
            column,
        })
    }
    
    /// Peek at the next character without consuming it
    fn peek_char(&mut self) -> Option<char> {
        self.chars.peek().copied()
    }
    
    /// Get the next character
    fn next_char(&mut self) -> Option<char> {
        if let Some(ch) = self.chars.next() {
            if ch == '\n' {
                self.line += 1;
                self.column = 1;
            } else {
                self.column += 1;
            }
            Some(ch)
        } else {
            None
        }
    }
    
    /// Skip whitespace and comments
    fn skip_whitespace_and_comments(&mut self) -> Result<()> {
        loop {
            match self.peek_char() {
                Some(' ') | Some('\t') | Some('\r') | Some('\n') => {
                    self.next_char();
                }
                Some('-') => {
                    // Peek ahead to check for comment
                    let mut chars_copy = self.chars.clone();
                    chars_copy.next(); // skip first -
                    if chars_copy.peek() == Some(&'-') {
                        // It's a comment
                        self.next_char(); // consume first -
                        self.next_char(); // consume second -
                        self.skip_comment()?;
                    } else {
                        // Not a comment, stop
                        break;
                    }
                }
                _ => break,
            }
        }
        Ok(())
    }
    
    /// Skip a comment
    fn skip_comment(&mut self) -> Result<()> {
        // Check for long comment
        if self.peek_char() == Some('[') {
            let mut chars_copy = self.chars.clone();
            chars_copy.next(); // skip [
            if chars_copy.peek() == Some(&'[') {
                // Long comment
                self.next_char(); // consume first [
                self.next_char(); // consume second [
                self.skip_long_comment()?;
                return Ok(());
            }
        }
        
        // Short comment - skip to end of line
        while let Some(ch) = self.peek_char() {
            if ch == '\n' {
                break;
            }
            self.next_char();
        }
        Ok(())
    }
    
    /// Skip a long comment
    fn skip_long_comment(&mut self) -> Result<()> {
        // Skip until ]]
        let mut bracket_count = 0;
        while let Some(ch) = self.next_char() {
            if ch == ']' {
                if self.peek_char() == Some(']') {
                    self.next_char();
                    return Ok(());
                }
            }
        }
        
        Err(LuaError::SyntaxError {
            message: "Unterminated long comment".to_string(),
            line: self.line as usize,
            column: self.column as usize,
        })
    }
    
    /// Read a number
    fn read_number(&mut self, first: char) -> Result<TokenType> {
        let mut number = String::new();
        number.push(first);
        
        // Read integer part
        while let Some(ch) = self.peek_char() {
            if ch.is_ascii_digit() {
                number.push(ch);
                self.next_char();
            } else {
                break;
            }
        }
        
        // Check for decimal point
        if self.peek_char() == Some('.') {
            let mut chars_copy = self.chars.clone();
            chars_copy.next(); // skip .
            if chars_copy.peek().map(|c| c.is_ascii_digit()).unwrap_or(false) {
                // It's a decimal number
                number.push('.');
                self.next_char();
                
                // Read fractional part
                while let Some(ch) = self.peek_char() {
                    if ch.is_ascii_digit() {
                        number.push(ch);
                        self.next_char();
                    } else {
                        break;
                    }
                }
            }
        }
        
        // Check for exponent
        if let Some('e') | Some('E') = self.peek_char() {
            number.push('e');
            self.next_char();
            
            // Check for sign
            if let Some('+') | Some('-') = self.peek_char() {
                number.push(self.next_char().unwrap());
            }
            
            // Read exponent digits
            let mut has_digits = false;
            while let Some(ch) = self.peek_char() {
                if ch.is_ascii_digit() {
                    number.push(ch);
                    self.next_char();
                    has_digits = true;
                } else {
                    break;
                }
            }
            
            if !has_digits {
                return Err(LuaError::SyntaxError {
                    message: "Malformed number".to_string(),
                    line: self.line as usize,
                    column: self.column as usize,
                });
            }
        }
        
        // Parse the number
        match number.parse::<f64>() {
            Ok(n) => Ok(TokenType::Number(n)),
            Err(_) => Err(LuaError::SyntaxError {
                message: format!("Invalid number: {}", number),
                line: self.line as usize,
                column: self.column as usize,
            }),
        }
    }
    
    /// Read a number starting with decimal point
    fn read_number_fraction(&mut self) -> Result<TokenType> {
        let mut number = String::from("0.");
        
        // Read fractional part
        while let Some(ch) = self.peek_char() {
            if ch.is_ascii_digit() {
                number.push(ch);
                self.next_char();
            } else {
                break;
            }
        }
        
        // Check for exponent
        if let Some('e') | Some('E') = self.peek_char() {
            number.push('e');
            self.next_char();
            
            // Check for sign
            if let Some('+') | Some('-') = self.peek_char() {
                number.push(self.next_char().unwrap());
            }
            
            // Read exponent digits
            let mut has_digits = false;
            while let Some(ch) = self.peek_char() {
                if ch.is_ascii_digit() {
                    number.push(ch);
                    self.next_char();
                    has_digits = true;
                } else {
                    break;
                }
            }
            
            if !has_digits {
                return Err(LuaError::SyntaxError {
                    message: "Malformed number".to_string(),
                    line: self.line as usize,
                    column: self.column as usize,
                });
            }
        }
        
        // Parse the number
        match number.parse::<f64>() {
            Ok(n) => Ok(TokenType::Number(n)),
            Err(_) => Err(LuaError::SyntaxError {
                message: format!("Invalid number: {}", number),
                line: self.line as usize,
                column: self.column as usize,
            }),
        }
    }
    
    /// Read a string literal
    fn read_string(&mut self, quote: char) -> Result<TokenType> {
        let mut string = String::new();
        
        loop {
            match self.next_char() {
                Some(ch) if ch == quote => {
                    // End of string
                    return Ok(TokenType::String(string));
                }
                Some('\\') => {
                    // Escape sequence
                    match self.next_char() {
                        Some('a') => string.push('\x07'), // bell
                        Some('b') => string.push('\x08'), // backspace
                        Some('f') => string.push('\x0C'), // form feed
                        Some('n') => string.push('\n'),   // newline
                        Some('r') => string.push('\r'),   // carriage return
                        Some('t') => string.push('\t'),   // tab
                        Some('v') => string.push('\x0B'), // vertical tab
                        Some('\\') => string.push('\\'),
                        Some('"') => string.push('"'),
                        Some('\'') => string.push('\''),
                        Some('\n') => string.push('\n'), // escaped newline
                        Some(ch) if ch.is_ascii_digit() => {
                            // Decimal escape sequence
                            let mut value = ch.to_digit(10).unwrap() as u8;
                            let mut count = 1;
                            
                            while count < 3 {
                                if let Some(ch) = self.peek_char() {
                                    if ch.is_ascii_digit() {
                                        self.next_char();
                                        value = value * 10 + ch.to_digit(10).unwrap() as u8;
                                        count += 1;
                                    } else {
                                        break;
                                    }
                                } else {
                                    break;
                                }
                            }
                            
                            string.push(value as char);
                        }
                        Some(ch) => {
                            return Err(LuaError::SyntaxError {
                                message: format!("Invalid escape sequence: \\{}", ch),
                                line: self.line as usize,
                                column: self.column as usize,
                            });
                        }
                        None => {
                            return Err(LuaError::SyntaxError {
                                message: "Unterminated string".to_string(),
                                line: self.line as usize,
                                column: self.column as usize,
                            });
                        }
                    }
                }
                Some(ch) => string.push(ch),
                None => {
                    return Err(LuaError::SyntaxError {
                        message: "Unterminated string".to_string(),
                        line: self.line as usize,
                        column: self.column as usize,
                    });
                }
            }
        }
    }
    
    /// Read a long string
    fn read_long_string(&mut self) -> Result<TokenType> {
        // We already saw the first [, check for second
        if self.next_char() != Some('[') {
            return Err(LuaError::SyntaxError {
                message: "Invalid long string".to_string(),
                line: self.line as usize,
                column: self.column as usize,
            });
        }
        
        let mut string = String::new();
        
        // Skip initial newline if present
        if self.peek_char() == Some('\n') {
            self.next_char();
        }
        
        // Read until ]]
        loop {
            match self.next_char() {
                Some(']') => {
                    if self.peek_char() == Some(']') {
                        self.next_char();
                        return Ok(TokenType::String(string));
                    } else {
                        string.push(']');
                    }
                }
                Some(ch) => string.push(ch),
                None => {
                    return Err(LuaError::SyntaxError {
                        message: "Unterminated long string".to_string(),
                        line: self.line as usize,
                        column: self.column as usize,
                    });
                }
            }
        }
    }
    
    /// Read an identifier or keyword
    fn read_identifier(&mut self, first: char) -> Result<TokenType> {
        let mut ident = String::new();
        ident.push(first);
        
        while let Some(ch) = self.peek_char() {
            if ch.is_alphanumeric() || ch == '_' {
                ident.push(ch);
                self.next_char();
            } else {
                break;
            }
        }
        
        // Check if it's a keyword
        let token_type = match ident.as_str() {
            "and" => TokenType::And,
            "break" => TokenType::Break,
            "do" => TokenType::Do,
            "else" => TokenType::Else,
            "elseif" => TokenType::Elseif,
            "end" => TokenType::End,
            "false" => TokenType::False,
            "for" => TokenType::For,
            "function" => TokenType::Function,
            "if" => TokenType::If,
            "in" => TokenType::In,
            "local" => TokenType::Local,
            "nil" => TokenType::Nil,
            "not" => TokenType::Not,
            "or" => TokenType::Or,
            "repeat" => TokenType::Repeat,
            "return" => TokenType::Return,
            "then" => TokenType::Then,
            "true" => TokenType::True,
            "until" => TokenType::Until,
            "while" => TokenType::While,
            _ => TokenType::Identifier(ident),
        };
        
        Ok(token_type)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_simple_tokens() {
        let mut lexer = Lexer::new("+ - * / % ^");
        assert!(matches!(lexer.next_token().unwrap().token_type, TokenType::Plus));
        assert!(matches!(lexer.next_token().unwrap().token_type, TokenType::Minus));
        assert!(matches!(lexer.next_token().unwrap().token_type, TokenType::Multiply));
        assert!(matches!(lexer.next_token().unwrap().token_type, TokenType::Divide));
        assert!(matches!(lexer.next_token().unwrap().token_type, TokenType::Modulo));
        assert!(matches!(lexer.next_token().unwrap().token_type, TokenType::Power));
        assert!(matches!(lexer.next_token().unwrap().token_type, TokenType::EOF));
    }
    
    #[test]
    fn test_numbers() {
        let mut lexer = Lexer::new("42 3.14 .5 1e10 1.5e-5");
        assert!(matches!(lexer.next_token().unwrap().token_type, TokenType::Number(42.0)));
        assert!(matches!(lexer.next_token().unwrap().token_type, TokenType::Number(3.14)));
        assert!(matches!(lexer.next_token().unwrap().token_type, TokenType::Number(0.5)));
        assert!(matches!(lexer.next_token().unwrap().token_type, TokenType::Number(1e10)));
        assert!(matches!(lexer.next_token().unwrap().token_type, TokenType::Number(1.5e-5)));
    }
    
    #[test]
    fn test_strings() {
        let mut lexer = Lexer::new(r#"'hello' "world" 'escape\ntest' [[long string]]"#);
        
        if let TokenType::String(s) = lexer.next_token().unwrap().token_type {
            assert_eq!(s, "hello");
        } else {
            panic!("Expected string");
        }
        
        if let TokenType::String(s) = lexer.next_token().unwrap().token_type {
            assert_eq!(s, "world");
        } else {
            panic!("Expected string");
        }
        
        if let TokenType::String(s) = lexer.next_token().unwrap().token_type {
            assert_eq!(s, "escape\ntest");
        } else {
            panic!("Expected string");
        }
        
        if let TokenType::String(s) = lexer.next_token().unwrap().token_type {
            assert_eq!(s, "long string");
        } else {
            panic!("Expected string");
        }
    }
}