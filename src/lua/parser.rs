//! Lua Parser Implementation
//!
//! This module implements a parser for the Lua language. It converts
//! Lua source code into an abstract syntax tree (AST) for compilation.

use std::fmt;

use super::error::{LuaError, Result, syntax_error};
use super::value::StringHandle;

/// A node in the abstract syntax tree
#[derive(Debug, Clone)]
pub struct Node<T> {
    /// The node's value
    pub value: T,
    
    /// Line number in source
    pub line: usize,
    
    /// Column number in source
    pub column: usize,
}

impl<T> Node<T> {
    /// Create a new node
    pub fn new(value: T, line: usize, column: usize) -> Self {
        Node { value, line, column }
    }
}

/// An expression in the AST
#[derive(Debug, Clone)]
pub enum Expression {
    /// Nil literal
    Nil,
    
    /// Boolean literal
    Boolean(bool),
    
    /// Number literal
    Number(f64),
    
    /// String literal
    String(StringHandle),
    
    /// Variable name
    Variable(StringHandle),
    
    /// Table constructor
    Table(Vec<TableField>),
    
    /// Function call
    Call {
        /// Function expression
        func: Box<Node<Expression>>,
        
        /// Arguments
        args: Vec<Node<Expression>>,
    },
    
    /// Method call (obj:method)
    MethodCall {
        /// Object expression
        object: Box<Node<Expression>>,
        
        /// Method name
        method: StringHandle,
        
        /// Arguments
        args: Vec<Node<Expression>>,
    },
    
    /// Field access (obj.field)
    FieldAccess {
        /// Object expression
        object: Box<Node<Expression>>,
        
        /// Field name
        field: StringHandle,
    },
    
    /// Index access (obj[index])
    IndexAccess {
        /// Object expression
        object: Box<Node<Expression>>,
        
        /// Index expression
        index: Box<Node<Expression>>,
    },
    
    /// Function definition
    Function {
        /// Parameter names
        params: Vec<StringHandle>,
        
        /// Function body
        body: Vec<Node<Statement>>,
        
        /// Is this a vararg function?
        is_vararg: bool,
    },
    
    /// Binary operation
    Binary {
        /// Operation type
        op: BinaryOp,
        
        /// Left operand
        left: Box<Node<Expression>>,
        
        /// Right operand
        right: Box<Node<Expression>>,
    },
    
    /// Unary operation
    Unary {
        /// Operation type
        op: UnaryOp,
        
        /// Operand
        operand: Box<Node<Expression>>,
    },
    
    /// Vararg expression (...)
    Vararg,
}

/// A table field in a table constructor
#[derive(Debug, Clone)]
pub enum TableField {
    /// Array entry ([index] = value)
    Array(Node<Expression>),
    
    /// Hash entry (key = value)
    Hash {
        /// Key expression
        key: Node<Expression>,
        
        /// Value expression
        value: Node<Expression>,
    },
    
    /// Field entry (name = value)
    Field {
        /// Field name
        name: StringHandle,
        
        /// Field value
        value: Node<Expression>,
    },
}

/// Binary operations
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BinaryOp {
    /// Addition (+)
    Add,
    
    /// Subtraction (-)
    Sub,
    
    /// Multiplication (*)
    Mul,
    
    /// Division (/)
    Div,
    
    /// Modulo (%)
    Mod,
    
    /// Power (^)
    Pow,
    
    /// Concatenation (..)
    Concat,
    
    /// Equality (==)
    Eq,
    
    /// Inequality (~=)
    Ne,
    
    /// Less than (<)
    Lt,
    
    /// Less than or equal (<=)
    Le,
    
    /// Greater than (>)
    Gt,
    
    /// Greater than or equal (>=)
    Ge,
    
    /// Logical and (and)
    And,
    
    /// Logical or (or)
    Or,
}

impl fmt::Display for BinaryOp {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            BinaryOp::Add => write!(f, "+"),
            BinaryOp::Sub => write!(f, "-"),
            BinaryOp::Mul => write!(f, "*"),
            BinaryOp::Div => write!(f, "/"),
            BinaryOp::Mod => write!(f, "%"),
            BinaryOp::Pow => write!(f, "^"),
            BinaryOp::Concat => write!(f, ".."),
            BinaryOp::Eq => write!(f, "=="),
            BinaryOp::Ne => write!(f, "~="),
            BinaryOp::Lt => write!(f, "<"),
            BinaryOp::Le => write!(f, "<="),
            BinaryOp::Gt => write!(f, ">"),
            BinaryOp::Ge => write!(f, ">="),
            BinaryOp::And => write!(f, "and"),
            BinaryOp::Or => write!(f, "or"),
        }
    }
}

/// Unary operations
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UnaryOp {
    /// Unary minus (-)
    Minus,
    
    /// Logical not (not)
    Not,
    
    /// Length operator (#)
    Len,
}

impl fmt::Display for UnaryOp {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            UnaryOp::Minus => write!(f, "-"),
            UnaryOp::Not => write!(f, "not "),
            UnaryOp::Len => write!(f, "#"),
        }
    }
}

/// A statement in the AST
#[derive(Debug, Clone)]
pub enum Statement {
    /// Assignment statement
    Assignment {
        /// Variables to assign to
        variables: Vec<Node<LValue>>,
        
        /// Expressions to assign
        expressions: Vec<Node<Expression>>,
    },
    
    /// Local variable declaration
    LocalDecl {
        /// Variable names
        names: Vec<StringHandle>,
        
        /// Initial values
        values: Vec<Node<Expression>>,
    },
    
    /// Function call statement
    Call(Node<Expression>),
    
    /// Do block
    Do(Vec<Node<Statement>>),
    
    /// While loop
    While {
        /// Condition expression
        condition: Node<Expression>,
        
        /// Loop body
        body: Vec<Node<Statement>>,
    },
    
    /// Repeat loop
    Repeat {
        /// Loop body
        body: Vec<Node<Statement>>,
        
        /// Until condition
        condition: Node<Expression>,
    },
    
    /// If statement
    If {
        /// Condition expression
        condition: Node<Expression>,
        
        /// Then block
        then_block: Vec<Node<Statement>>,
        
        /// Elseif clauses
        elseif_clauses: Vec<(Node<Expression>, Vec<Node<Statement>>)>,
        
        /// Else block
        else_block: Option<Vec<Node<Statement>>>,
    },
    
    /// Numeric for loop
    ForNum {
        /// Loop variable name
        var: StringHandle,
        
        /// Start expression
        start: Node<Expression>,
        
        /// Limit expression
        limit: Node<Expression>,
        
        /// Step expression
        step: Option<Node<Expression>>,
        
        /// Loop body
        body: Vec<Node<Statement>>,
    },
    
    /// Generic for loop
    ForIn {
        /// Loop variable names
        vars: Vec<StringHandle>,
        
        /// Iterator expressions
        iterators: Vec<Node<Expression>>,
        
        /// Loop body
        body: Vec<Node<Statement>>,
    },
    
    /// Function definition
    FunctionDef {
        /// Function name
        name: Node<FunctionName>,
        
        /// Parameter names
        params: Vec<StringHandle>,
        
        /// Function body
        body: Vec<Node<Statement>>,
        
        /// Is this a vararg function?
        is_vararg: bool,
        
        /// Is this a local function?
        is_local: bool,
    },
    
    /// Return statement
    Return(Vec<Node<Expression>>),
    
    /// Break statement
    Break,
}

/// A function name (possibly with table fields and method)
#[derive(Debug, Clone)]
pub enum FunctionName {
    /// Simple name
    Name(StringHandle),
    
    /// Table field path
    Path {
        /// Base name
        base: StringHandle,
        
        /// Field names
        fields: Vec<StringHandle>,
    },
    
    /// Method
    Method {
        /// Base name
        base: StringHandle,
        
        /// Field names
        fields: Vec<StringHandle>,
        
        /// Method name
        method: StringHandle,
    },
}

/// An L-value (something that can be assigned to)
#[derive(Debug, Clone)]
pub enum LValue {
    /// Variable name
    Name(StringHandle),
    
    /// Field access (obj.field)
    FieldAccess {
        /// Object expression
        object: Box<Node<Expression>>,
        
        /// Field name
        field: StringHandle,
    },
    
    /// Index access (obj[index])
    IndexAccess {
        /// Object expression
        object: Box<Node<Expression>>,
        
        /// Index expression
        index: Box<Node<Expression>>,
    },
}

/// A token in the Lua lexer
#[derive(Debug, Clone, PartialEq)]
pub enum Token {
    /// End of file
    Eof,
    
    /// Identifier
    Ident(String),
    
    /// String literal
    String(String),
    
    /// Number literal
    Number(f64),
    
    /// Keywords
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
    
    /// Operators
    Plus,
    Minus,
    Star,
    Slash,
    Percent,
    Caret,
    Pound,
    Equal,
    NotEqual,
    LessThan,
    LessEqual,
    GreaterThan,
    GreaterEqual,
    Assign,
    Concat,
    
    /// Delimiters
    Semicolon,
    Comma,
    Dot,
    Colon,
    LeftParen,
    RightParen,
    LeftBracket,
    RightBracket,
    LeftBrace,
    RightBrace,
    
    /// Vararg
    Dots,
}

/// The parser state
pub struct Parser {
    /// The lexer
    lexer: Lexer,
    
    /// The current token
    current: Token,
    
    /// Current line number
    line: usize,
    
    /// Current column number
    column: usize,
    
    /// String interner - used to create StringHandles
    interner: StringInterner,
}

/// A simplified lexer implementation
pub struct Lexer {
    /// Source code
    source: Vec<char>,
    
    /// Current position
    position: usize,
    
    /// Current line
    line: usize,
    
    /// Current column
    column: usize,
}

/// A string interner for the parser
#[derive(Default)]
pub struct StringInterner(pub Vec<String>);

impl StringInterner {
    /// Create a new string interner
    pub fn new() -> Self {
        StringInterner(Vec::new())
    }
    
    /// Intern a string
    pub fn intern(&mut self, s: &str) -> StringHandle {
        for (i, existing) in self.0.iter().enumerate() {
            if existing == s {
                return StringHandle(super::arena::Handle {
                    index: i as u32,
                    generation: 0,
                    _phantom: std::marker::PhantomData,
                });
            }
        }
        
        let index = self.0.len();
        self.0.push(s.to_string());
        
        StringHandle(super::arena::Handle {
            index: index as u32,
            generation: 0,
            _phantom: std::marker::PhantomData,
        })
    }
    
    /// Get string by handle
    pub fn get(&self, handle: StringHandle) -> Option<&str> {
        self.0.get(handle.0.index as usize).map(|s| s.as_str())
    }
}

impl Lexer {
    /// Create a new lexer
    pub fn new(source: &str) -> Self {
        Lexer {
            source: source.chars().collect(),
            position: 0,
            line: 1,
            column: 1,
        }
    }
    
    /// Get the current character
    fn current(&self) -> Option<char> {
        if self.position < self.source.len() {
            Some(self.source[self.position])
        } else {
            None
        }
    }
    
    /// Advance to the next character
    fn advance(&mut self) {
        if let Some(c) = self.current() {
            if c == '\n' {
                self.line += 1;
                self.column = 1;
            } else {
                self.column += 1;
            }
            self.position += 1;
        }
    }
    
    /// Skip whitespace and comments
    fn skip_whitespace_and_comments(&mut self) {
        loop {
            match self.current() {
                Some(c) if c.is_whitespace() => {
                    self.advance();
                },
                Some('-') if self.peek() == Some('-') => {
                    // Comment
                    self.advance(); // Skip -
                    self.advance(); // Skip -
                    
                    // Skip until end of line or EOF
                    while let Some(c) = self.current() {
                        if c == '\n' {
                            self.advance();
                            break;
                        }
                        self.advance();
                    }
                },
                _ => break,
            }
        }
    }
    
    /// Peek at the next character
    fn peek(&self) -> Option<char> {
        if self.position + 1 < self.source.len() {
            Some(self.source[self.position + 1])
        } else {
            None
        }
    }
    
    /// Read an identifier or keyword
    fn read_identifier(&mut self) -> String {
        let mut result = String::new();
        
        while let Some(c) = self.current() {
            if c.is_alphanumeric() || c == '_' {
                result.push(c);
                self.advance();
            } else {
                break;
            }
        }
        
        result
    }
    
    /// Read a number
    fn read_number(&mut self) -> Result<f64> {
        let mut result = String::new();
        let mut has_decimal = false;
        
        while let Some(c) = self.current() {
            if c.is_digit(10) {
                result.push(c);
                self.advance();
            } else if c == '.' && !has_decimal {
                has_decimal = true;
                result.push(c);
                self.advance();
            } else if c == 'e' || c == 'E' {
                // Handle scientific notation
                result.push(c);
                self.advance();
                
                // Optional sign
                if let Some(sign) = self.current() {
                    if sign == '+' || sign == '-' {
                        result.push(sign);
                        self.advance();
                    }
                }
                
                // Require at least one digit
                if let Some(c) = self.current() {
                    if !c.is_digit(10) {
                        return Err(syntax_error("malformed number", self.line, self.column));
                    }
                } else {
                    return Err(syntax_error("malformed number", self.line, self.column));
                }
                
                // Read exponent
                while let Some(c) = self.current() {
                    if c.is_digit(10) {
                        result.push(c);
                        self.advance();
                    } else {
                        break;
                    }
                }
            } else {
                break;
            }
        }
        
        // Parse the number
        match result.parse::<f64>() {
            Ok(n) => Ok(n),
            Err(_) => Err(syntax_error("malformed number", self.line, self.column)),
        }
    }
    
    /// Read a string
    fn read_string(&mut self, delimiter: char) -> Result<String> {
        // Skip the opening delimiter
        self.advance();
        
        let mut result = String::new();
        
        while let Some(c) = self.current() {
            if c == delimiter {
                // Skip the closing delimiter
                self.advance();
                return Ok(result);
            } else if c == '\\' {
                // Handle escape sequences
                self.advance();
                
                match self.current() {
                    Some('a') => { result.push('\x07'); self.advance(); },
                    Some('b') => { result.push('\x08'); self.advance(); },
                    Some('f') => { result.push('\x0C'); self.advance(); },
                    Some('n') => { result.push('\n'); self.advance(); },
                    Some('r') => { result.push('\r'); self.advance(); },
                    Some('t') => { result.push('\t'); self.advance(); },
                    Some('v') => { result.push('\x0B'); self.advance(); },
                    Some('\\') => { result.push('\\'); self.advance(); },
                    Some('\'') => { result.push('\''); self.advance(); },
                    Some('"') => { result.push('"'); self.advance(); },
                    Some('\n') => { result.push('\n'); self.advance(); },
                    Some(c) => {
                        // Just add the character as-is
                        result.push(c);
                        self.advance();
                    },
                    None => {
                        return Err(syntax_error("unexpected EOF in string", self.line, self.column));
                    },
                }
            } else {
                result.push(c);
                self.advance();
            }
        }
        
        Err(syntax_error("unexpected EOF in string", self.line, self.column))
    }
    
    /// Read a long string [[ ... ]]
    fn read_long_string(&mut self) -> Result<String> {
        // Skip the opening [[
        self.advance();
        self.advance();
        
        // Skip an optional newline at the beginning
        if self.current() == Some('\n') {
            self.advance();
        }
        
        let mut result = String::new();
        
        while let Some(c) = self.current() {
            if c == ']' && self.peek() == Some(']') {
                // Potential end of string
                self.advance();
                self.advance();
                return Ok(result);
            } else {
                result.push(c);
                self.advance();
            }
        }
        
        Err(syntax_error("unexpected EOF in long string", self.line, self.column))
    }
    
    /// Get the next token
    pub fn next_token(&mut self) -> Result<(Token, usize, usize)> {
        // Skip whitespace and comments
        self.skip_whitespace_and_comments();
        
        // Save current position
        let line = self.line;
        let column = self.column;
        
        // Check for EOF
        if self.position >= self.source.len() {
            return Ok((Token::Eof, line, column));
        }
        
        // Get current character
        let c = self.source[self.position];
        
        // Identify tokens
        match c {
            // Single-character tokens
            '+' => { self.advance(); Ok((Token::Plus, line, column)) },
            '-' => { self.advance(); Ok((Token::Minus, line, column)) },
            '*' => { self.advance(); Ok((Token::Star, line, column)) },
            '/' => { self.advance(); Ok((Token::Slash, line, column)) },
            '%' => { self.advance(); Ok((Token::Percent, line, column)) },
            '^' => { self.advance(); Ok((Token::Caret, line, column)) },
            '#' => { self.advance(); Ok((Token::Pound, line, column)) },
            ';' => { self.advance(); Ok((Token::Semicolon, line, column)) },
            ',' => { self.advance(); Ok((Token::Comma, line, column)) },
            '(' => { self.advance(); Ok((Token::LeftParen, line, column)) },
            ')' => { self.advance(); Ok((Token::RightParen, line, column)) },
            '[' => { 
                self.advance();
                // Check for long string
                if self.current() == Some('[') {
                    let content = self.read_long_string()?;
                    Ok((Token::String(content), line, column))
                } else {
                    Ok((Token::LeftBracket, line, column))
                }
            },
            ']' => { self.advance(); Ok((Token::RightBracket, line, column)) },
            '{' => { self.advance(); Ok((Token::LeftBrace, line, column)) },
            '}' => { self.advance(); Ok((Token::RightBrace, line, column)) },
            
            // Two-character tokens
            '=' => {
                self.advance();
                if self.current() == Some('=') {
                    self.advance();
                    Ok((Token::Equal, line, column))
                } else {
                    Ok((Token::Assign, line, column))
                }
            },
            '~' => {
                self.advance();
                if self.current() == Some('=') {
                    self.advance();
                    Ok((Token::NotEqual, line, column))
                } else {
                    Err(syntax_error("expected '=' after '~'", line, column))
                }
            },
            '<' => {
                self.advance();
                if self.current() == Some('=') {
                    self.advance();
                    Ok((Token::LessEqual, line, column))
                } else {
                    Ok((Token::LessThan, line, column))
                }
            },
            '>' => {
                self.advance();
                if self.current() == Some('=') {
                    self.advance();
                    Ok((Token::GreaterEqual, line, column))
                } else {
                    Ok((Token::GreaterThan, line, column))
                }
            },
            '.' => {
                self.advance();
                if self.current() == Some('.') {
                    self.advance();
                    if self.current() == Some('.') {
                        self.advance();
                        Ok((Token::Dots, line, column))
                    } else {
                        Ok((Token::Concat, line, column))
                    }
                } else {
                    Ok((Token::Dot, line, column))
                }
            },
            ':' => {
                self.advance();
                if self.current() == Some(':') {
                    self.advance();
                    // Label token (not used in this simplified parser)
                    // For now, just return colon
                    Ok((Token::Colon, line, column))
                } else {
                    Ok((Token::Colon, line, column))
                }
            },
            
            // Strings
            '"' | '\'' => {
                let content = self.read_string(c)?;
                Ok((Token::String(content), line, column))
            },
            
            // Numbers
            '0'..='9' => {
                let number = self.read_number()?;
                Ok((Token::Number(number), line, column))
            },
            
            // Identifiers and keywords
            c if c.is_alphabetic() || c == '_' => {
                let ident = self.read_identifier();
                
                // Check for keywords
                let token = match ident.as_str() {
                    "and" => Token::And,
                    "break" => Token::Break,
                    "do" => Token::Do,
                    "else" => Token::Else,
                    "elseif" => Token::Elseif,
                    "end" => Token::End,
                    "false" => Token::False,
                    "for" => Token::For,
                    "function" => Token::Function,
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
                    _ => Token::Ident(ident),
                };
                
                Ok((token, line, column))
            },
            
            // Unexpected character
            _ => {
                Err(syntax_error(&format!("unexpected character '{}'", c), line, column))
            },
        }
    }
}

impl Parser {
    /// Create a new parser
    pub fn new(source: &str) -> Self {
        let mut lexer = Lexer::new(source);
        let (current, line, column) = lexer.next_token().unwrap_or((Token::Eof, 1, 1));
        
        Parser {
            lexer,
            current,
            line,
            column,
            interner: StringInterner::new(),
        }
    }
    
    /// Advance to the next token
    fn advance(&mut self) -> Result<()> {
        let (token, line, column) = self.lexer.next_token()?;
        self.current = token;
        self.line = line;
        self.column = column;
        Ok(())
    }
    
    /// Expect a token
    fn expect(&mut self, expected: Token) -> Result<()> {
        if matches!(&self.current, e if std::mem::discriminant(e) == std::mem::discriminant(&expected)) {
            self.advance()?;
            Ok(())
        } else {
            Err(syntax_error(&format!("expected {:?}, got {:?}", expected, self.current), self.line, self.column))
        }
    }
    
    /// Parse a Lua chunk
    pub fn parse(&mut self) -> Result<Vec<Node<Statement>>> {
        let mut statements = Vec::new();
        
        while self.current != Token::Eof {
            let stmt = self.parse_statement()?;
            statements.push(stmt);
            
            // Skip optional semicolons
            if self.current == Token::Semicolon {
                self.advance()?;
            }
        }
        
        Ok(statements)
    }
    
    /// Parse a statement
    fn parse_statement(&mut self) -> Result<Node<Statement>> {
        let line = self.line;
        let column = self.column;
        
        match self.current {
            Token::Function => {
                self.advance()?;
                self.parse_function_def(false, line, column)
            },
            Token::Local => {
                self.advance()?;
                if self.current == Token::Function {
                    self.advance()?;
                    self.parse_function_def(true, line, column)
                } else {
                    self.parse_local_declaration(line, column)
                }
            },
            Token::If => self.parse_if_statement(),
            Token::While => self.parse_while_statement(),
            Token::Do => self.parse_do_statement(),
            Token::Repeat => self.parse_repeat_statement(),
            Token::For => self.parse_for_statement(),
            Token::Return => self.parse_return_statement(),
            Token::Break => {
                self.advance()?;
                Ok(Node::new(Statement::Break, line, column))
            },
            _ => {
                // Try to parse a function call or assignment
                self.parse_call_or_assignment()
            },
        }
    }
    
    /// Parse a function definition
    fn parse_function_def(&mut self, is_local: bool, line: usize, column: usize) -> Result<Node<Statement>> {
        // Parse function name
        let name = self.parse_function_name(is_local)?;
        
        // Parse parameters
        self.expect(Token::LeftParen)?;
        
        let mut params = Vec::new();
        let mut is_vararg = false;
        
        if self.current != Token::RightParen {
            loop {
                if self.current == Token::Dots {
                    is_vararg = true;
                    self.advance()?;
                    break;
                }
                
                if let Token::Ident(ident) = &self.current {
                    params.push(self.interner.intern(ident));
                    self.advance()?;
                    
                    if self.current == Token::Comma {
                        self.advance()?;
                    } else {
                        break;
                    }
                } else {
                    return Err(syntax_error("expected parameter name", self.line, self.column));
                }
            }
        }
        
        self.expect(Token::RightParen)?;
        
        // Parse body
        let body = self.parse_block()?;
        
        self.expect(Token::End)?;
        
        // Create function definition
        Ok(Node::new(Statement::FunctionDef {
            name: Node::new(name, line, column),
            params,
            body,
            is_vararg,
            is_local,
        }, line, column))
    }
    
    /// Parse a function name
    fn parse_function_name(&mut self, is_local: bool) -> Result<FunctionName> {
        if is_local {
            if let Token::Ident(ident) = &self.current {
                let name = self.interner.intern(ident);
                self.advance()?;
                return Ok(FunctionName::Name(name));
            } else {
                return Err(syntax_error("expected function name", self.line, self.column));
            }
        }
        
        // Parse base name
        if let Token::Ident(ident) = &self.current {
            let base = self.interner.intern(ident);
            self.advance()?;
            
            // Parse fields
            let mut fields = Vec::new();
            let mut is_method = false;
            let mut method = None;
            
            while self.current == Token::Dot || self.current == Token::Colon {
                is_method = self.current == Token::Colon;
                self.advance()?;
                
                if let Token::Ident(ident) = &self.current {
                    let field = self.interner.intern(ident);
                    self.advance()?;
                    
                    if is_method {
                        method = Some(field);
                        break;
                    } else {
                        fields.push(field);
                    }
                } else {
                    return Err(syntax_error("expected field name", self.line, self.column));
                }
            }
            
            // Create function name
            if let Some(method) = method {
                Ok(FunctionName::Method {
                    base,
                    fields,
                    method,
                })
            } else if fields.is_empty() {
                Ok(FunctionName::Name(base))
            } else {
                Ok(FunctionName::Path {
                    base,
                    fields,
                })
            }
        } else {
            Err(syntax_error("expected function name", self.line, self.column))
        }
    }
    
    /// Parse a block of statements
    fn parse_block(&mut self) -> Result<Vec<Node<Statement>>> {
        let mut statements = Vec::new();
        
        while !matches!(self.current, Token::End | Token::Else | Token::Elseif | Token::Until | Token::Eof) {
            let stmt = self.parse_statement()?;
            statements.push(stmt);
            
            // Skip optional semicolons
            if self.current == Token::Semicolon {
                self.advance()?;
            }
        }
        
        Ok(statements)
    }
    
    /// Parse a local declaration
    fn parse_local_declaration(&mut self, line: usize, column: usize) -> Result<Node<Statement>> {
        // Parse variable names
        let mut names = Vec::new();
        
        if let Token::Ident(ident) = &self.current {
            names.push(self.interner.intern(ident));
            self.advance()?;
            
            while self.current == Token::Comma {
                self.advance()?;
                
                if let Token::Ident(ident) = &self.current {
                    names.push(self.interner.intern(ident));
                    self.advance()?;
                } else {
                    return Err(syntax_error("expected variable name", self.line, self.column));
                }
            }
        } else {
            return Err(syntax_error("expected variable name", self.line, self.column));
        }
        
        // Parse initial values
        let mut values = Vec::new();
        
        if self.current == Token::Assign {
            self.advance()?;
            
            // Parse expression list
            values.push(self.parse_expression()?);
            
            while self.current == Token::Comma {
                self.advance()?;
                values.push(self.parse_expression()?);
            }
        }
        
        Ok(Node::new(Statement::LocalDecl {
            names,
            values,
        }, line, column))
    }
    
    /// Parse an if statement
    fn parse_if_statement(&mut self) -> Result<Node<Statement>> {
        let line = self.line;
        let column = self.column;
        
        self.advance()?; // Skip 'if'
        
        // Parse condition
        let condition = self.parse_expression()?;
        
        self.expect(Token::Then)?;
        
        // Parse then block
        let then_block = self.parse_block()?;
        
        // Parse elseif clauses
        let mut elseif_clauses = Vec::new();
        
        while self.current == Token::Elseif {
            let elseif_line = self.line;
            let elseif_column = self.column;
            
            self.advance()?; // Skip 'elseif'
            
            // Parse condition
            let elseif_condition = self.parse_expression()?;
            
            self.expect(Token::Then)?;
            
            // Parse block
            let elseif_block = self.parse_block()?;
            
            elseif_clauses.push((
                Node::new(elseif_condition.value, elseif_line, elseif_column),
                elseif_block,
            ));
        }
        
        // Parse else clause
        let else_block = if self.current == Token::Else {
            self.advance()?; // Skip 'else'
            
            Some(self.parse_block()?)
        } else {
            None
        };
        
        self.expect(Token::End)?;
        
        Ok(Node::new(Statement::If {
            condition,
            then_block,
            elseif_clauses,
            else_block,
        }, line, column))
    }
    
    /// Parse a while statement
    fn parse_while_statement(&mut self) -> Result<Node<Statement>> {
        let line = self.line;
        let column = self.column;
        
        self.advance()?; // Skip 'while'
        
        // Parse condition
        let condition = self.parse_expression()?;
        
        self.expect(Token::Do)?;
        
        // Parse body
        let body = self.parse_block()?;
        
        self.expect(Token::End)?;
        
        Ok(Node::new(Statement::While {
            condition,
            body,
        }, line, column))
    }
    
    /// Parse a do statement
    fn parse_do_statement(&mut self) -> Result<Node<Statement>> {
        let line = self.line;
        let column = self.column;
        
        self.advance()?; // Skip 'do'
        
        // Parse body
        let body = self.parse_block()?;
        
        self.expect(Token::End)?;
        
        Ok(Node::new(Statement::Do(body), line, column))
    }
    
    /// Parse a repeat statement
    fn parse_repeat_statement(&mut self) -> Result<Node<Statement>> {
        let line = self.line;
        let column = self.column;
        
        self.advance()?; // Skip 'repeat'
        
        // Parse body
        let body = self.parse_block()?;
        
        self.expect(Token::Until)?;
        
        // Parse condition
        let condition = self.parse_expression()?;
        
        Ok(Node::new(Statement::Repeat {
            body,
            condition,
        }, line, column))
    }
    
    /// Parse a for statement
    fn parse_for_statement(&mut self) -> Result<Node<Statement>> {
        let line = self.line;
        let column = self.column;
        
        self.advance()?; // Skip 'for'
        
        // Parse variable name
        if let Token::Ident(ident) = &self.current {
            let var = self.interner.intern(ident);
            self.advance()?;
            
            // Check if numeric or generic for
            if self.current == Token::Assign {
                // Numeric for
                self.advance()?; // Skip '='
                
                // Parse start
                let start = self.parse_expression()?;
                
                self.expect(Token::Comma)?;
                
                // Parse limit
                let limit = self.parse_expression()?;
                
                // Parse optional step
                let step = if self.current == Token::Comma {
                    self.advance()?;
                    Some(self.parse_expression()?)
                } else {
                    None
                };
                
                self.expect(Token::Do)?;
                
                // Parse body
                let body = self.parse_block()?;
                
                self.expect(Token::End)?;
                
                Ok(Node::new(Statement::ForNum {
                    var,
                    start,
                    limit,
                    step,
                    body,
                }, line, column))
            } else if self.current == Token::Comma || self.current == Token::In {
                // Generic for
                let mut vars = vec![var];
                
                // Parse additional variables
                while self.current == Token::Comma {
                    self.advance()?;
                    
                    if let Token::Ident(ident) = &self.current {
                        vars.push(self.interner.intern(ident));
                        self.advance()?;
                    } else {
                        return Err(syntax_error("expected variable name", self.line, self.column));
                    }
                }
                
                self.expect(Token::In)?;
                
                // Parse iterators
                let mut iterators = Vec::new();
                iterators.push(self.parse_expression()?);
                
                while self.current == Token::Comma {
                    self.advance()?;
                    iterators.push(self.parse_expression()?);
                }
                
                self.expect(Token::Do)?;
                
                // Parse body
                let body = self.parse_block()?;
                
                self.expect(Token::End)?;
                
                Ok(Node::new(Statement::ForIn {
                    vars,
                    iterators,
                    body,
                }, line, column))
            } else {
                Err(syntax_error("expected '=' or 'in' after variable name", self.line, self.column))
            }
        } else {
            Err(syntax_error("expected variable name", self.line, self.column))
        }
    }
    
    /// Parse a return statement
    fn parse_return_statement(&mut self) -> Result<Node<Statement>> {
        let line = self.line;
        let column = self.column;
        
        self.advance()?; // Skip 'return'
        
        // Parse optional expressions
        let mut expressions = Vec::new();
        
        if !matches!(self.current, Token::End | Token::Else | Token::Elseif | Token::Until | Token::Eof | Token::Semicolon) {
            expressions.push(self.parse_expression()?);
            
            while self.current == Token::Comma {
                self.advance()?;
                expressions.push(self.parse_expression()?);
            }
        }
        
        Ok(Node::new(Statement::Return(expressions), line, column))
    }
    
    /// Parse a function call or assignment
    fn parse_call_or_assignment(&mut self) -> Result<Node<Statement>> {
        let line = self.line;
        let column = self.column;
        
        // Parse prefixexp
        let var = self.parse_prefixexp()?;
        
        // Check if it's a call
        if matches!(var.value, Expression::Call { .. } | Expression::MethodCall { .. }) {
            return Ok(Node::new(Statement::Call(var), line, column));
        }
        
        // Must be an assignment
        let mut vars = Vec::new();
        
        // Convert expression to LValue
        let lvalue = match var.value {
            Expression::Variable(name) => {
                LValue::Name(name)
            },
            Expression::FieldAccess { object, field } => {
                LValue::FieldAccess {
                    object,
                    field,
                }
            },
            Expression::IndexAccess { object, index } => {
                LValue::IndexAccess {
                    object,
                    index,
                }
            },
            _ => {
                return Err(syntax_error("invalid left-hand side of assignment", line, column));
            }
        };
        
        vars.push(Node::new(lvalue, var.line, var.column));
        
        // Parse more variables
        while self.current == Token::Comma {
            self.advance()?;
            
            let var = self.parse_prefixexp()?;
            
            // Convert expression to LValue
            let lvalue = match var.value {
                Expression::Variable(name) => {
                    LValue::Name(name)
                },
                Expression::FieldAccess { object, field } => {
                    LValue::FieldAccess {
                        object,
                        field,
                    }
                },
                Expression::IndexAccess { object, index } => {
                    LValue::IndexAccess {
                        object,
                        index,
                    }
                },
                _ => {
                    return Err(syntax_error("invalid left-hand side of assignment", var.line, var.column));
                }
            };
            
            vars.push(Node::new(lvalue, var.line, var.column));
        }
        
        // Parse assignment
        self.expect(Token::Assign)?;
        
        // Parse expressions
        let mut expressions = Vec::new();
        expressions.push(self.parse_expression()?);
        
        while self.current == Token::Comma {
            self.advance()?;
            expressions.push(self.parse_expression()?);
        }
        
        Ok(Node::new(Statement::Assignment {
            variables: vars,
            expressions,
        }, line, column))
    }
    
    /// Parse an expression
    fn parse_expression(&mut self) -> Result<Node<Expression>> {
        self.parse_subexpr(0)
    }
    
    /// Parse a subexpression with a minimum precedence
    fn parse_subexpr(&mut self, min_prec: i32) -> Result<Node<Expression>> {
        let line = self.line;
        let column = self.column;
        
        // Parse prefix expression
        let mut expr = match self.current {
            Token::Nil => {
                self.advance()?;
                Node::new(Expression::Nil, line, column)
            },
            Token::True => {
                self.advance()?;
                Node::new(Expression::Boolean(true), line, column)
            },
            Token::False => {
                self.advance()?;
                Node::new(Expression::Boolean(false), line, column)
            },
            Token::Number(n) => {
                let value = n;
                self.advance()?;
                Node::new(Expression::Number(value), line, column)
            },
            Token::String(ref s) => {
                let value = s.clone();
                self.advance()?;
                Node::new(Expression::String(self.interner.intern(&value)), line, column)
            },
            Token::Dots => {
                self.advance()?;
                Node::new(Expression::Vararg, line, column)
            },
            Token::LeftBrace => {
                self.advance()?;
                self.parse_table(line, column)?
            },
            Token::Function => {
                self.advance()?;
                self.parse_function_expr(line, column)?
            },
            Token::LeftParen => {
                self.advance()?;
                let expr = self.parse_expression()?;
                self.expect(Token::RightParen)?;
                expr
            },
            Token::Minus | Token::Not | Token::Pound => {
                let op = match self.current {
                    Token::Minus => UnaryOp::Minus,
                    Token::Not => UnaryOp::Not,
                    Token::Pound => UnaryOp::Len,
                    _ => unreachable!(),
                };
                self.advance()?;
                let operand = self.parse_subexpr(8)?; // Unary operators have precedence 8
                Node::new(Expression::Unary {
                    op,
                    operand: Box::new(operand),
                }, line, column)
            },
            _ => {
                self.parse_prefixexp()?
            },
        };
        
        // Parse binary operators
        loop {
            let op_prec = match self.current {
                Token::Plus => 6,
                Token::Minus => 6,
                Token::Star => 7,
                Token::Slash => 7,
                Token::Percent => 7,
                Token::Caret => 10, // Right associative
                Token::Concat => 5,
                Token::Equal => 3,
                Token::NotEqual => 3,
                Token::LessThan => 3,
                Token::LessEqual => 3,
                Token::GreaterThan => 3,
                Token::GreaterEqual => 3,
                Token::And => 2,
                Token::Or => 1,
                _ => -1,
            };
            
            if op_prec < min_prec {
                break;
            }
            
            let op = match self.current {
                Token::Plus => BinaryOp::Add,
                Token::Minus => BinaryOp::Sub,
                Token::Star => BinaryOp::Mul,
                Token::Slash => BinaryOp::Div,
                Token::Percent => BinaryOp::Mod,
                Token::Caret => BinaryOp::Pow,
                Token::Concat => BinaryOp::Concat,
                Token::Equal => BinaryOp::Eq,
                Token::NotEqual => BinaryOp::Ne,
                Token::LessThan => BinaryOp::Lt,
                Token::LessEqual => BinaryOp::Le,
                Token::GreaterThan => BinaryOp::Gt,
                Token::GreaterEqual => BinaryOp::Ge,
                Token::And => BinaryOp::And,
                Token::Or => BinaryOp::Or,
                _ => break,
            };
            
            self.advance()?;
            
            // For right-associative operators (^), subtract 1 from precedence
            let next_min_prec = if op == BinaryOp::Pow {
                op_prec - 1
            } else {
                op_prec
            };
            
            let right = self.parse_subexpr(next_min_prec)?;
            
            expr = Node::new(Expression::Binary {
                op,
                left: Box::new(expr),
                right: Box::new(right),
            }, line, column);
        }
        
        Ok(expr)
    }
    
    /// Parse a prefix expression (variable, function call, etc.)
    fn parse_prefixexp(&mut self) -> Result<Node<Expression>> {
        let line = self.line;
        let column = self.column;
        
        // Parse primary
        let mut expr = match self.current {
            Token::Ident(ref ident) => {
                let name = self.interner.intern(ident);
                self.advance()?;
                Node::new(Expression::Variable(name), line, column)
            },
            Token::LeftParen => {
                self.advance()?;
                let expr = self.parse_expression()?;
                self.expect(Token::RightParen)?;
                expr
            },
            _ => {
                return Err(syntax_error("unexpected token in expression", self.line, self.column));
            },
        };
        
        // Parse suffixes
        loop {
            match self.current {
                Token::Dot => {
                    self.advance()?;
                    
                    // Parse field name
                    if let Token::Ident(ref ident) = self.current {
                        let field = self.interner.intern(ident);
                        self.advance()?;
                        
                        expr = Node::new(Expression::FieldAccess {
                            object: Box::new(expr),
                            field,
                        }, line, column);
                    } else {
                        return Err(syntax_error("expected field name", self.line, self.column));
                    }
                },
                Token::LeftBracket => {
                    self.advance()?;
                    
                    // Parse index
                    let index = self.parse_expression()?;
                    
                    self.expect(Token::RightBracket)?;
                    
                    expr = Node::new(Expression::IndexAccess {
                        object: Box::new(expr),
                        index: Box::new(index),
                    }, line, column);
                },
                Token::Colon => {
                    self.advance()?;
                    
                    // Parse method name
                    if let Token::Ident(ref ident) = self.current {
                        let method = self.interner.intern(ident);
                        self.advance()?;
                        
                        // Parse arguments
                        let args = self.parse_args()?;
                        
                        expr = Node::new(Expression::MethodCall {
                            object: Box::new(expr),
                            method,
                            args,
                        }, line, column);
                    } else {
                        return Err(syntax_error("expected method name", self.line, self.column));
                    }
                },
                Token::LeftParen | Token::String(_) | Token::LeftBrace => {
                    // Function call with arguments
                    let args = self.parse_args()?;
                    
                    expr = Node::new(Expression::Call {
                        func: Box::new(expr),
                        args,
                    }, line, column);
                },
                _ => break,
            }
        }
        
        Ok(expr)
    }
    
    /// Parse function arguments
    fn parse_args(&mut self) -> Result<Vec<Node<Expression>>> {
        let mut args = Vec::new();
        
        match self.current {
            Token::LeftParen => {
                self.advance()?;
                
                if self.current != Token::RightParen {
                    // Parse argument list
                    args.push(self.parse_expression()?);
                    
                    while self.current == Token::Comma {
                        self.advance()?;
                        args.push(self.parse_expression()?);
                    }
                }
                
                self.expect(Token::RightParen)?;
            },
            Token::String(ref s) => {
                // String literal as argument
                let value = s.clone();
                args.push(Node::new(Expression::String(self.interner.intern(&value)), self.line, self.column));
                self.advance()?;
            },
            Token::LeftBrace => {
                // Table constructor as argument
                let line = self.line;
                let column = self.column;
                args.push(self.parse_table(line, column)?);
            },
            _ => {
                return Err(syntax_error("expected function arguments", self.line, self.column));
            },
        }
        
        Ok(args)
    }
    
    /// Parse a table constructor
    fn parse_table(&mut self, line: usize, column: usize) -> Result<Node<Expression>> {
        // Already consumed the '{'
        
        let mut fields = Vec::new();
        
        // Check for empty table
        if self.current == Token::RightBrace {
            self.advance()?;
            return Ok(Node::new(Expression::Table(fields), line, column));
        }
        
        // Parse fields
        loop {
            // Parse field
            match self.current {
                Token::LeftBracket => {
                    // Hash field: [key] = value
                    self.advance()?;
                    
                    let key = self.parse_expression()?;
                    
                    self.expect(Token::RightBracket)?;
                    self.expect(Token::Assign)?;
                    
                    let value = self.parse_expression()?;
                    
                    fields.push(TableField::Hash {
                        key,
                        value,
                    });
                },
                Token::Ident(ref ident) => {
                    let name = self.interner.intern(ident);
                    self.advance()?;
                    
                    if self.current == Token::Assign {
                        // Field: name = value
                        self.advance()?;
                        
                        let value = self.parse_expression()?;
                        
                        fields.push(TableField::Field {
                            name,
                            value,
                        });
                    } else {
                        // Array entry: value (using previously parsed ident)
                        let value = Node::new(Expression::Variable(name), line, column);
                        
                        fields.push(TableField::Array(value));
                    }
                },
                _ => {
                    // Array entry: value
                    let value = self.parse_expression()?;
                    
                    fields.push(TableField::Array(value));
                }
            }
            
            // Check for field separator
            if self.current == Token::Comma || self.current == Token::Semicolon {
                self.advance()?;
            } else {
                break;
            }
            
            // Check for end of table
            if self.current == Token::RightBrace {
                break;
            }
        }
        
        self.expect(Token::RightBrace)?;
        
        Ok(Node::new(Expression::Table(fields), line, column))
    }
    
    /// Parse a function expression
    fn parse_function_expr(&mut self, line: usize, column: usize) -> Result<Node<Expression>> {
        // Already consumed 'function'
        
        // Parse parameters
        self.expect(Token::LeftParen)?;
        
        let mut params = Vec::new();
        let mut is_vararg = false;
        
        if self.current != Token::RightParen {
            loop {
                if self.current == Token::Dots {
                    is_vararg = true;
                    self.advance()?;
                    break;
                }
                
                if let Token::Ident(ref ident) = self.current {
                    params.push(self.interner.intern(ident));
                    self.advance()?;
                    
                    if self.current == Token::Comma {
                        self.advance()?;
                    } else {
                        break;
                    }
                } else {
                    return Err(syntax_error("expected parameter name", self.line, self.column));
                }
            }
        }
        
        self.expect(Token::RightParen)?;
        
        // Parse body
        let body = self.parse_block()?;
        
        self.expect(Token::End)?;
        
        Ok(Node::new(Expression::Function {
            params,
            body,
            is_vararg,
        }, line, column))
    }
    
    /// Get the string interner
    pub fn get_interner(self) -> StringInterner {
        self.interner
    }
}