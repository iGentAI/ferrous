//! Lua Lexer Module
//!
//! This module implements a lexer for Lua source code, converting it into tokens
//! that can be processed by the parser.

use std::fmt;
use super::error::{LuaError, LuaResult};

/// Token type representing all Lua lexical tokens
#[derive(Debug, Clone, PartialEq)]
pub enum Token {
    // Keywords
    And, Break, Do, Else, ElseIf, End, 
    False, For, Function, Goto, If, In, 
    Local, Nil, Not, Or, Repeat, Return,
    Then, True, Until, While,
    
    // Operators
    Plus, Minus, Mul, Div, Mod, Pow,
    Concat, Equal, NotEqual, LessThan,
    LessEqual, GreaterThan, GreaterEqual,
    Assign, Hash, Len,
    
    // Punctuation
    Semicolon, Colon, DoubleColon,
    Comma, Dot, DoubleDot, TripleDot,
    LeftParen, RightParen,
    LeftBracket, RightBracket,
    LeftBrace, RightBrace,
    
    // Literals
    Number(f64),
    String(String),
    
    // Identifiers
    Identifier(String),
    
    // End of file
    Eof,
}

impl fmt::Display for Token {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Token::And => write!(f, "and"),
            Token::Break => write!(f, "break"),
            Token::Do => write!(f, "do"),
            Token::Else => write!(f, "else"),
            Token::ElseIf => write!(f, "elseif"),
            Token::End => write!(f, "end"),
            Token::False => write!(f, "false"),
            Token::For => write!(f, "for"),
            Token::Function => write!(f, "function"),
            Token::Goto => write!(f, "goto"),
            Token::If => write!(f, "if"),
            Token::In => write!(f, "in"),
            Token::Local => write!(f, "local"),
            Token::Nil => write!(f, "nil"),
            Token::Not => write!(f, "not"),
            Token::Or => write!(f, "or"),
            Token::Repeat => write!(f, "repeat"),
            Token::Return => write!(f, "return"),
            Token::Then => write!(f, "then"),
            Token::True => write!(f, "true"),
            Token::Until => write!(f, "until"),
            Token::While => write!(f, "while"),
            
            Token::Plus => write!(f, "+"),
            Token::Minus => write!(f, "-"),
            Token::Mul => write!(f, "*"),
            Token::Div => write!(f, "/"),
            Token::Mod => write!(f, "%"),
            Token::Pow => write!(f, "^"),
            Token::Concat => write!(f, ".."),
            Token::Equal => write!(f, "=="),
            Token::NotEqual => write!(f, "~="),
            Token::LessThan => write!(f, "<"),
            Token::LessEqual => write!(f, "<="),
            Token::GreaterThan => write!(f, ">"),
            Token::GreaterEqual => write!(f, ">="),
            Token::Assign => write!(f, "="),
            Token::Hash => write!(f, "#"),
            Token::Len => write!(f, "#"),
            
            Token::Semicolon => write!(f, ";"),
            Token::Colon => write!(f, ":"),
            Token::DoubleColon => write!(f, "::"),
            Token::Comma => write!(f, ","),
            Token::Dot => write!(f, "."),
            Token::DoubleDot => write!(f, ".."),
            Token::TripleDot => write!(f, "..."),
            Token::LeftParen => write!(f, "("),
            Token::RightParen => write!(f, ")"),
            Token::LeftBracket => write!(f, "["),
            Token::RightBracket => write!(f, "]"),
            Token::LeftBrace => write!(f, "{{"),
            Token::RightBrace => write!(f, "}}"),
            
            Token::Number(n) => write!(f, "{}", n),
            Token::String(s) => write!(f, "\"{}\"", s.escape_debug()),
            Token::Identifier(s) => write!(f, "{}", s),
            
            Token::Eof => write!(f, "EOF"),
        }
    }
}

/// Token with location information
#[derive(Debug, Clone)]
pub struct TokenWithLocation {
    /// The token itself
    pub token: Token,
    /// Line number (1-based)
    pub line: usize,
    /// Column number (1-based)
    pub column: usize,
}

/// Lexer for Lua source code
pub struct Lexer<'a> {
    /// Source code
    source: &'a str,
    /// Current position in source
    position: usize,
    /// Current line number (1-based)
    line: usize,
    /// Current column number (1-based)
    column: usize,
    /// Characters of source code
    chars: Vec<char>,
    /// End of file reached
    eof: bool,
}

impl<'a> Lexer<'a> {
    /// Create a new lexer
    pub fn new(source: &'a str) -> Self {
        let chars = source.chars().collect();
        Lexer {
            source,
            position: 0,
            line: 1,
            column: 1,
            chars,
            eof: false,
        }
    }
    
    /// Get the current character
    fn current(&self) -> Option<char> {
        if self.position < self.chars.len() {
            Some(self.chars[self.position])
        } else {
            None
        }
    }
    
    /// Peek at the next character
    fn peek(&self) -> Option<char> {
        if self.position + 1 < self.chars.len() {
            Some(self.chars[self.position + 1])
        } else {
            None
        }
    }
    
    /// Advance to the next character
    fn advance(&mut self) {
        if let Some(c) = self.current() {
            self.position += 1;
            if c == '\n' {
                self.line += 1;
                self.column = 1;
            } else {
                self.column += 1;
            }
        } else {
            self.eof = true;
        }
    }
    
    /// Skip whitespace
    fn skip_whitespace(&mut self) {
        while let Some(c) = self.current() {
            if c.is_whitespace() {
                self.advance();
            } else {
                break;
            }
        }
    }
    
    /// Skip a comment
    fn skip_comment(&mut self) {
        // Check for comment start
        if self.current() != Some('-') || self.peek() != Some('-') {
            return;
        }
        
        // Skip --
        self.advance();
        self.advance();
        
        // Check for long comment
        let mut is_long = false;
        let mut level = 0;
        
        if self.current() == Some('[') {
            self.advance();
            // Count equals
            while self.current() == Some('=') {
                level += 1;
                self.advance();
            }
            
            if self.current() == Some('[') {
                is_long = true;
                self.advance();
            } else {
                // Not a long comment, rewind
                level = 0;
            }
        }
        
        if is_long {
            // Skip until end of long comment
            let mut found = false;
            while !self.eof && !found {
                if self.current() == Some(']') {
                    self.advance();
                    
                    // Count equals
                    let mut count = 0;
                    while self.current() == Some('=') {
                        count += 1;
                        self.advance();
                    }
                    
                    if count == level && self.current() == Some(']') {
                        self.advance();
                        found = true;
                    }
                } else {
                    self.advance();
                }
            }
        } else {
            // Skip until end of line
            while let Some(c) = self.current() {
                self.advance();
                if c == '\n' {
                    break;
                }
            }
        }
    }
    
    /// Parse a number
    fn parse_number(&mut self) -> LuaResult<Token> {
        let start = self.position;
        let mut has_decimal = false;
        let mut has_exponent = false;
        
        // Parse integer part
        while let Some(c) = self.current() {
            if c.is_ascii_digit() {
                self.advance();
            } else if c == '.' && !has_decimal && !has_exponent {
                has_decimal = true;
                self.advance();
            } else if (c == 'e' || c == 'E') && !has_exponent {
                has_exponent = true;
                self.advance();
                
                // Optional sign
                if let Some(c) = self.current() {
                    if c == '+' || c == '-' {
                        self.advance();
                    }
                }
            } else {
                break;
            }
        }
        
        // Extract number string
        let num_str = self.chars[start..self.position].iter().collect::<String>();
        
        // Parse the number
        match num_str.parse::<f64>() {
            Ok(n) => Ok(Token::Number(n)),
            Err(_) => Err(LuaError::SyntaxError {
                message: format!("Invalid number: {}", num_str),
                line: self.line,
                column: self.column - num_str.len(),
            })
        }
    }
    
    /// Parse a string
    fn parse_string(&mut self) -> LuaResult<Token> {
        let delimiter = self.current().unwrap();
        self.advance(); // Skip the opening quote
        
        let mut result = String::new();
        let start_line = self.line;
        let start_column = self.column;
        
        while let Some(c) = self.current() {
            if c == delimiter {
                self.advance(); // Skip the closing quote
                return Ok(Token::String(result));
            } else if c == '\\' {
                self.advance(); // Skip the backslash
                
                match self.current() {
                    Some('a') => { result.push('\u{07}'); self.advance(); }
                    Some('b') => { result.push('\u{08}'); self.advance(); }
                    Some('f') => { result.push('\u{0C}'); self.advance(); }
                    Some('n') => { result.push('\n'); self.advance(); }
                    Some('r') => { result.push('\r'); self.advance(); }
                    Some('t') => { result.push('\t'); self.advance(); }
                    Some('v') => { result.push('\u{0B}'); self.advance(); }
                    Some('\\') => { result.push('\\'); self.advance(); }
                    Some('"') => { result.push('"'); self.advance(); }
                    Some('\'') => { result.push('\''); self.advance(); }
                    Some('\n') => { result.push('\n'); self.advance(); }
                    Some('z') => { 
                        self.advance(); 
                        // Skip whitespace
                        self.skip_whitespace();
                    }
                    Some(c) if c.is_ascii_digit() => {
                        // Decimal escape
                        let mut value = 0;
                        for _ in 0..3 {
                            if let Some(d) = self.current() {
                                if d.is_ascii_digit() {
                                    value = value * 10 + (d as u8 - b'0');
                                    self.advance();
                                } else {
                                    break;
                                }
                            } else {
                                break;
                            }
                        }
                        result.push(char::from(value));
                    }
                    Some('x') => {
                        // Hex escape
                        self.advance();
                        let mut value = 0;
                        for _ in 0..2 {
                            if let Some(d) = self.current() {
                                if d.is_ascii_hexdigit() {
                                    value = value * 16 + d.to_digit(16).unwrap() as u8;
                                    self.advance();
                                } else {
                                    break;
                                }
                            } else {
                                break;
                            }
                        }
                        result.push(char::from(value));
                    }
                    Some(c) => {
                        // Invalid escape
                        return Err(LuaError::SyntaxError {
                            message: format!("Invalid escape sequence: \\{}", c),
                            line: self.line,
                            column: self.column - 1,
                        });
                    }
                    None => {
                        // Unexpected end of file
                        return Err(LuaError::SyntaxError {
                            message: "Unfinished string".to_string(),
                            line: start_line,
                            column: start_column,
                        });
                    }
                }
            } else if c == '\n' && delimiter != '[' {
                // Newlines aren't allowed in regular strings
                return Err(LuaError::SyntaxError {
                    message: "Unfinished string".to_string(),
                    line: start_line,
                    column: start_column,
                });
            } else {
                result.push(c);
                self.advance();
            }
        }
        
        // If we get here, we've reached the end of the file without a closing quote
        Err(LuaError::SyntaxError {
            message: "Unfinished string".to_string(),
            line: start_line,
            column: start_column,
        })
    }
    
    /// Parse a long string
    fn parse_long_string(&mut self) -> LuaResult<Token> {
        // Skip the opening bracket
        self.advance();
        
        // Count equals
        let mut level = 0;
        while self.current() == Some('=') {
            level += 1;
            self.advance();
        }
        
        // Make sure we have the second bracket
        if self.current() != Some('[') {
            return Err(LuaError::SyntaxError {
                message: "Invalid long string delimiter".to_string(),
                line: self.line,
                column: self.column,
            });
        }
        
        // Skip the opening bracket
        self.advance();
        
        // Skip initial newline if present
        if self.current() == Some('\n') {
            self.advance();
        }
        
        // Parse the string
        let mut result = String::new();
        let start_line = self.line;
        let start_column = self.column;
        
        while let Some(c) = self.current() {
            if c == ']' {
                self.advance();
                
                // Count equals
                let mut count = 0;
                while self.current() == Some('=') {
                    count += 1;
                    self.advance();
                }
                
                if count == level && self.current() == Some(']') {
                    self.advance();
                    return Ok(Token::String(result));
                } else {
                    // Not the right closing delimiter
                    result.push(']');
                    for _ in 0..count {
                        result.push('=');
                    }
                    if self.current() == Some(']') {
                        result.push(']');
                        self.advance();
                    }
                }
            } else {
                result.push(c);
                self.advance();
            }
        }
        
        // If we get here, we've reached the end of the file without a closing delimiter
        Err(LuaError::SyntaxError {
            message: "Unfinished long string".to_string(),
            line: start_line,
            column: start_column,
        })
    }
    
    /// Parse an identifier
    fn parse_identifier(&mut self) -> Token {
        let start = self.position;
        
        // Parse the identifier
        while let Some(c) = self.current() {
            if c.is_alphanumeric() || c == '_' {
                self.advance();
            } else {
                break;
            }
        }
        
        // Extract the identifier
        let ident = self.chars[start..self.position].iter().collect::<String>();
        
        // Check for keywords
        match ident.as_str() {
            "and" => Token::And,
            "break" => Token::Break,
            "do" => Token::Do,
            "else" => Token::Else,
            "elseif" => Token::ElseIf,
            "end" => Token::End,
            "false" => Token::False,
            "for" => Token::For,
            "function" => Token::Function,
            "goto" => Token::Goto,
            "if" => Token::If,
            "in" => Token::In,
            "local" => Token::Local,
            "nil" => Token::Nil,
            "not" => Token::Not,
            "or" => Token::Or,
            "repeat" => Token::Repeat,
            "return" => Token::Return,
            "then" => Token::Then,
            "true" => Token::True,
            "until" => Token::Until,
            "while" => Token::While,
            _ => Token::Identifier(ident),
        }
    }
    
    /// Get the next token
    pub fn next_token(&mut self) -> LuaResult<TokenWithLocation> {
        // Skip whitespace and comments
        loop {
            self.skip_whitespace();
            
            if self.current() == Some('-') && self.peek() == Some('-') {
                self.skip_comment();
            } else {
                break;
            }
        }
        
        // Remember the start position
        let line = self.line;
        let column = self.column;
        
        // Check for EOF
        if self.eof || self.current().is_none() {
            self.eof = true;
            return Ok(TokenWithLocation {
                token: Token::Eof,
                line,
                column,
            });
        }
        
        // Parse the token
        let token = match self.current().unwrap() {
            // Single character tokens
            '+' => { self.advance(); Token::Plus },
            '*' => { self.advance(); Token::Mul },
            '%' => { self.advance(); Token::Mod },
            '^' => { self.advance(); Token::Pow },
            '#' => { self.advance(); Token::Hash },
            ';' => { self.advance(); Token::Semicolon },
            ',' => { self.advance(); Token::Comma },
            '(' => { self.advance(); Token::LeftParen },
            ')' => { self.advance(); Token::RightParen },
            '[' => { 
                self.advance();
                // Check for long string
                if self.current() == Some('[') || 
                   (self.current() == Some('=') && 
                    self.chars[self.position..].iter()
                    .take_while(|&&c| c == '=')
                    .count() > 0 &&
                    self.chars.get(self.position + self.chars[self.position..].iter()
                                  .take_while(|&&c| c == '=').count()) == Some(&'[')) {
                    self.position -= 1; // Back up to parse long string correctly
                    self.column -= 1;
                    self.parse_long_string()?
                } else {
                    Token::LeftBracket
                }
            },
            ']' => { self.advance(); Token::RightBracket },
            '{' => { self.advance(); Token::LeftBrace },
            '}' => { self.advance(); Token::RightBrace },
            
            // Potentially multi-character tokens
            '-' => {
                self.advance();
                Token::Minus
            },
            '/' => {
                self.advance();
                Token::Div
            },
            '=' => {
                self.advance();
                if self.current() == Some('=') {
                    self.advance();
                    Token::Equal
                } else {
                    Token::Assign
                }
            },
            '~' => {
                self.advance();
                if self.current() == Some('=') {
                    self.advance();
                    Token::NotEqual
                } else {
                    return Err(LuaError::SyntaxError {
                        message: "Expected '=' after '~'".to_string(),
                        line,
                        column,
                    });
                }
            },
            '<' => {
                self.advance();
                if self.current() == Some('=') {
                    self.advance();
                    Token::LessEqual
                } else {
                    Token::LessThan
                }
            },
            '>' => {
                self.advance();
                if self.current() == Some('=') {
                    self.advance();
                    Token::GreaterEqual
                } else {
                    Token::GreaterThan
                }
            },
            ':' => {
                self.advance();
                if self.current() == Some(':') {
                    self.advance();
                    Token::DoubleColon
                } else {
                    Token::Colon
                }
            },
            '.' => {
                self.advance();
                if self.current() == Some('.') {
                    self.advance();
                    if self.current() == Some('.') {
                        self.advance();
                        Token::TripleDot
                    } else {
                        Token::DoubleDot
                    }
                } else if self.current().map_or(false, |c| c.is_ascii_digit()) {
                    // Number starting with a decimal point
                    self.position -= 1; // Back up to parse number correctly
                    self.column -= 1;
                    self.parse_number()?
                } else {
                    Token::Dot
                }
            },
            
            // String literals
            '\'' | '"' => self.parse_string()?,
            
            // Number literals
            c if c.is_ascii_digit() => self.parse_number()?,
            
            // Identifiers
            c if c.is_alphabetic() || c == '_' => self.parse_identifier(),
            
            // Invalid character
            c => {
                return Err(LuaError::SyntaxError {
                    message: format!("Unexpected character: {}", c),
                    line,
                    column,
                });
            }
        };
        
        Ok(TokenWithLocation {
            token,
            line,
            column,
        })
    }
    
    /// Get all tokens from the source
    pub fn tokenize(&mut self) -> LuaResult<Vec<TokenWithLocation>> {
        let mut tokens = Vec::new();
        
        loop {
            let token = self.next_token()?;
            let is_eof = matches!(token.token, Token::Eof);
            tokens.push(token);
            
            if is_eof {
                break;
            }
        }
        
        Ok(tokens)
    }
}

/// Tokenize a string into tokens
pub fn tokenize(source: &str) -> LuaResult<Vec<TokenWithLocation>> {
    let mut lexer = Lexer::new(source);
    lexer.tokenize()
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_simple_tokens() {
        let source = "local x = 42";
        let tokens = tokenize(source).unwrap();
        
        assert_eq!(tokens.len(), 5); // local, x, =, 42, EOF
        assert_eq!(tokens[0].token, Token::Local);
        assert_eq!(tokens[1].token, Token::Identifier("x".to_string()));
        assert_eq!(tokens[2].token, Token::Assign);
        assert_eq!(tokens[3].token, Token::Number(42.0));
        assert_eq!(tokens[4].token, Token::Eof);
    }
    
    #[test]
    fn test_string_tokens() {
        let source = "\"hello\\nworld\"";
        let tokens = tokenize(source).unwrap();
        
        assert_eq!(tokens.len(), 2); // string, EOF
        if let Token::String(s) = &tokens[0].token {
            assert_eq!(s, "hello\nworld");
        } else {
            panic!("Expected string token");
        }
    }
    
    #[test]
    fn test_comment() {
        let source = "-- This is a comment\nlocal x";
        let tokens = tokenize(source).unwrap();
        
        assert_eq!(tokens.len(), 3); // local, x, EOF
        assert_eq!(tokens[0].token, Token::Local);
    }
    
    #[test]
    fn test_operators() {
        let source = "+ - * / % ^ == ~= < <= > >= .. ...";
        let tokens = tokenize(source).unwrap();
        
        assert_eq!(tokens[0].token, Token::Plus);
        assert_eq!(tokens[1].token, Token::Minus);
        assert_eq!(tokens[2].token, Token::Mul);
        assert_eq!(tokens[3].token, Token::Div);
        assert_eq!(tokens[4].token, Token::Mod);
        assert_eq!(tokens[5].token, Token::Pow);
        assert_eq!(tokens[6].token, Token::Equal);
        assert_eq!(tokens[7].token, Token::NotEqual);
        assert_eq!(tokens[8].token, Token::LessThan);
        assert_eq!(tokens[9].token, Token::LessEqual);
        assert_eq!(tokens[10].token, Token::GreaterThan);
        assert_eq!(tokens[11].token, Token::GreaterEqual);
        assert_eq!(tokens[12].token, Token::DoubleDot);
        assert_eq!(tokens[13].token, Token::TripleDot);
    }
}