//! Lua Parser
//!
//! This module will implement a full Lua 5.1 parser that converts source code
//! into an abstract syntax tree for compilation.

use super::error::{LuaError, Result, syntax_error};
use super::value::StringHandle;
use super::arena::Handle;
use std::marker::PhantomData;
use std::collections::HashMap;

/// A node in the syntax tree with source location information
#[derive(Debug, Clone)]
pub struct Node<T> {
    /// The value of the node
    pub value: T,
    /// The line number where this node appears
    pub line: usize,
    /// The column number where this node appears
    pub column: usize,
}

/// A Lua statement
#[derive(Debug, Clone)]
pub enum Statement {
    /// Assignment: variables = expressions
    Assignment {
        /// Variables to assign to
        variables: Vec<Node<LValue>>,
        /// Expressions to assign
        expressions: Vec<Node<Expression>>,
    },
    
    /// Local declaration: local names = values
    LocalDecl {
        /// Variable names
        names: Vec<StringHandle>,
        /// Expressions to assign
        values: Vec<Node<Expression>>,
    },
    
    /// Function call
    Call(Node<Expression>),
    
    /// Do block: do ... end
    Do(Vec<Node<Statement>>),
    
    /// While loop: while condition do ... end
    While {
        /// Loop condition
        condition: Node<Expression>,
        /// Loop body
        body: Vec<Node<Statement>>,
    },
    
    /// Repeat loop: repeat ... until condition
    Repeat {
        /// Loop body
        body: Vec<Node<Statement>>,
        /// Loop condition
        condition: Node<Expression>,
    },
    
    /// If statement: if condition then ... elseif ... else ... end
    If {
        /// Main condition
        condition: Node<Expression>,
        /// Then block
        then_block: Vec<Node<Statement>>,
        /// Elseif clauses: [(condition, block), ...]
        elseif_clauses: Vec<(Node<Expression>, Vec<Node<Statement>>)>,
        /// Else block
        else_block: Option<Vec<Node<Statement>>>,
    },
    
    /// Numeric for loop: for var = start, limit, step do ... end
    ForNum {
        /// Loop variable
        var: StringHandle,
        /// Start expression
        start: Node<Expression>,
        /// Limit expression
        limit: Node<Expression>,
        /// Step expression (optional)
        step: Option<Node<Expression>>,
        /// Loop body
        body: Vec<Node<Statement>>,
    },
    
    /// Generic for loop: for vars in iterators do ... end
    ForIn {
        /// Loop variables
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
        /// Parameters
        params: Vec<StringHandle>,
        /// Function body
        body: Vec<Node<Statement>>,
        /// Is variadic?
        is_vararg: bool,
        /// Is local function?
        is_local: bool,
    },
    
    /// Return statement
    Return(Vec<Node<Expression>>),
    
    /// Break statement
    Break,
}

/// A Lua expression
#[derive(Debug, Clone)]
pub enum Expression {
    /// nil
    Nil,
    
    /// Boolean value
    Boolean(bool),
    
    /// Number value
    Number(f64),
    
    /// String value
    String(StringHandle),
    
    /// Variable reference
    Variable(StringHandle),
    
    /// Table constructor
    Table(Vec<TableField>),
    
    /// Function call
    Call {
        /// Function to call
        func: Box<Node<Expression>>,
        /// Arguments
        args: Vec<Node<Expression>>,
    },
    
    /// Method call (obj:method(...))
    MethodCall {
        /// Object
        object: Box<Node<Expression>>,
        /// Method name
        method: StringHandle,
        /// Arguments
        args: Vec<Node<Expression>>,
    },
    
    /// Field access (obj.field)
    FieldAccess {
        /// Object
        object: Box<Node<Expression>>,
        /// Field name
        field: StringHandle,
    },
    
    /// Index access (obj[index])
    IndexAccess {
        /// Object
        object: Box<Node<Expression>>,
        /// Index expression
        index: Box<Node<Expression>>,
    },
    
    /// Function expression
    Function {
        /// Parameters
        params: Vec<StringHandle>,
        /// Function body
        body: Vec<Node<Statement>>,
        /// Is variadic?
        is_vararg: bool,
    },
    
    /// Binary operation
    Binary {
        /// Operator
        op: BinaryOp,
        /// Left operand
        left: Box<Node<Expression>>,
        /// Right operand
        right: Box<Node<Expression>>,
    },
    
    /// Unary operation
    Unary {
        /// Operator
        op: UnaryOp,
        /// Operand
        operand: Box<Node<Expression>>,
    },
    
    /// Vararg expression (...)
    Vararg,
}

/// A binary operator
#[derive(Debug, Clone, Copy, PartialEq)]
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

/// A unary operator
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum UnaryOp {
    /// Unary minus (-)
    Minus,
    /// Logical not (not)
    Not,
    /// Length (#)
    Len,
}

/// A table field
#[derive(Debug, Clone)]
pub enum TableField {
    /// Array entry
    Array(Node<Expression>),
    
    /// Hash entry [key] = value
    Hash {
        /// Key expression
        key: Node<Expression>,
        /// Value expression
        value: Node<Expression>,
    },
    
    /// Field entry name = value
    Field {
        /// Field name
        name: StringHandle,
        /// Value expression
        value: Node<Expression>,
    },
}

/// A function name
#[derive(Debug, Clone)]
pub enum FunctionName {
    /// Simple name: function name() ... end
    Name(StringHandle),
    
    /// Path: function a.b.c() ... end
    Path {
        /// Base name
        base: StringHandle,
        /// Field path
        fields: Vec<StringHandle>,
    },
    
    /// Method: function a.b:c() ... end
    Method {
        /// Base name
        base: StringHandle,
        /// Field path
        fields: Vec<StringHandle>,
        /// Method name
        method: StringHandle,
    },
}

/// An L-value (left side of assignment)
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

/// Token type
#[derive(Debug, Clone, PartialEq)]
pub enum TokenType {
    /// End of file
    Eof,
    /// Identifier
    Ident(String),
    /// Number
    Number(f64),
    /// String
    String(String),
    /// Keyword
    Keyword(String),
    /// Operator
    Operator(String),
    /// Separator
    Separator(char),
}

/// A token with source location
#[derive(Debug, Clone)]
pub struct Token {
    /// Token type
    pub token_type: TokenType,
    /// Line number
    pub line: usize,
    /// Column number
    pub column: usize,
}

/// The Lua parser
pub struct Parser<'a> {
    /// Source code
    source: &'a str,
    /// Current tokens
    tokens: Vec<Token>,
    /// Current token index
    current: usize,
}

impl<'a> Parser<'a> {
    /// Create a new parser
    pub fn new(source: &'a str) -> Self {
        Parser {
            source,
            tokens: Vec::new(),
            current: 0,
        }
    }
    
    /// Parse the source code
    pub fn parse(&mut self) -> Result<Vec<Node<Statement>>> {
        // For now, we'll implement a very basic parser for simple scripts
        // A full parser would need a complete lexer and recursive descent parser
        
        // Tokenize the input (extremely simplified)
        let tokens = self.tokenize()?;
        
        // Parse tokens into AST
        self.parse_tokens(tokens)
    }
    
    /// Simple tokenizer for basic Lua syntax
    fn tokenize(&self) -> Result<Vec<Token>> {
        let mut tokens = Vec::new();
        let mut line = 1;
        let mut column = 1;
        let mut chars = self.source.chars().peekable();
        
        while let Some(c) = chars.next() {
            match c {
                // Skip whitespace
                c if c.is_whitespace() => {
                    if c == '\n' {
                        line += 1;
                        column = 1;
                    } else {
                        column += 1;
                    }
                },
                
                // Skip comments
                '-' if chars.peek() == Some(&'-') => {
                    // Consume the second dash
                    chars.next();
                    column += 2;
                    
                    // Skip until end of line
                    while let Some(c) = chars.next() {
                        if c == '\n' {
                            line += 1;
                            column = 1;
                            break;
                        }
                        column += 1;
                    }
                },
                
                // Identifiers and keywords
                c if c.is_alphabetic() || c == '_' => {
                    let start_column = column;
                    let mut ident = String::new();
                    ident.push(c);
                    column += 1;
                    
                    while let Some(&c) = chars.peek() {
                        if c.is_alphanumeric() || c == '_' {
                            ident.push(chars.next().unwrap());
                            column += 1;
                        } else {
                            break;
                        }
                    }
                    
                    // Check if it's a keyword
                    match ident.as_str() {
                        "and" => tokens.push(Token { token_type: TokenType::Keyword(ident), line, column: start_column }),
                        "break" => tokens.push(Token { token_type: TokenType::Keyword(ident), line, column: start_column }),
                        "do" => tokens.push(Token { token_type: TokenType::Keyword(ident), line, column: start_column }),
                        "else" => tokens.push(Token { token_type: TokenType::Keyword(ident), line, column: start_column }),
                        "elseif" => tokens.push(Token { token_type: TokenType::Keyword(ident), line, column: start_column }),
                        "end" => tokens.push(Token { token_type: TokenType::Keyword(ident), line, column: start_column }),
                        "false" => tokens.push(Token { token_type: TokenType::Keyword(ident), line, column: start_column }),
                        "for" => tokens.push(Token { token_type: TokenType::Keyword(ident), line, column: start_column }),
                        "function" => tokens.push(Token { token_type: TokenType::Keyword(ident), line, column: start_column }),
                        "if" => tokens.push(Token { token_type: TokenType::Keyword(ident), line, column: start_column }),
                        "in" => tokens.push(Token { token_type: TokenType::Keyword(ident), line, column: start_column }),
                        "local" => tokens.push(Token { token_type: TokenType::Keyword(ident), line, column: start_column }),
                        "nil" => tokens.push(Token { token_type: TokenType::Keyword(ident), line, column: start_column }),
                        "not" => tokens.push(Token { token_type: TokenType::Keyword(ident), line, column: start_column }),
                        "or" => tokens.push(Token { token_type: TokenType::Keyword(ident), line, column: start_column }),
                        "repeat" => tokens.push(Token { token_type: TokenType::Keyword(ident), line, column: start_column }),
                        "return" => tokens.push(Token { token_type: TokenType::Keyword(ident), line, column: start_column }),
                        "then" => tokens.push(Token { token_type: TokenType::Keyword(ident), line, column: start_column }),
                        "true" => tokens.push(Token { token_type: TokenType::Keyword(ident), line, column: start_column }),
                        "until" => tokens.push(Token { token_type: TokenType::Keyword(ident), line, column: start_column }),
                        "while" => tokens.push(Token { token_type: TokenType::Keyword(ident), line, column: start_column }),
                        _ => tokens.push(Token { token_type: TokenType::Ident(ident), line, column: start_column }),
                    }
                },
                
                // Numbers
                c if c.is_digit(10) => {
                    let start_column = column;
                    let mut num = String::new();
                    num.push(c);
                    column += 1;
                    
                    // Integer part
                    while let Some(&c) = chars.peek() {
                        if c.is_digit(10) {
                            num.push(chars.next().unwrap());
                            column += 1;
                        } else {
                            break;
                        }
                    }
                    
                    // Decimal part
                    if chars.peek() == Some(&'.') {
                        num.push(chars.next().unwrap());
                        column += 1;
                        
                        while let Some(&c) = chars.peek() {
                            if c.is_digit(10) {
                                num.push(chars.next().unwrap());
                                column += 1;
                            } else {
                                break;
                            }
                        }
                    }
                    
                    // Exponent part
                    if chars.peek() == Some(&'e') || chars.peek() == Some(&'E') {
                        num.push(chars.next().unwrap());
                        column += 1;
                        
                        // Optional sign
                        if chars.peek() == Some(&'+') || chars.peek() == Some(&'-') {
                            num.push(chars.next().unwrap());
                            column += 1;
                        }
                        
                        // Exponent digits
                        while let Some(&c) = chars.peek() {
                            if c.is_digit(10) {
                                num.push(chars.next().unwrap());
                                column += 1;
                            } else {
                                break;
                            }
                        }
                    }
                    
                    // Parse number
                    match num.parse::<f64>() {
                        Ok(n) => tokens.push(Token { token_type: TokenType::Number(n), line, column: start_column }),
                        Err(_) => return Err(syntax_error("invalid number", line, start_column)),
                    }
                },
                
                // Strings
                '"' | '\'' => {
                    let start_column = column;
                    let delimiter = c;
                    let mut str_value = String::new();
                    column += 1;
                    
                    while let Some(c) = chars.next() {
                        column += 1;
                        
                        if c == delimiter {
                            break;
                        } else if c == '\\' {
                            // Escape sequence
                            if let Some(c) = chars.next() {
                                column += 1;
                                
                                match c {
                                    'n' => str_value.push('\n'),
                                    't' => str_value.push('\t'),
                                    'r' => str_value.push('\r'),
                                    '\\' => str_value.push('\\'),
                                    '"' => str_value.push('"'),
                                    '\'' => str_value.push('\''),
                                    _ => str_value.push(c), // Other escape sequences not handled
                                }
                            } else {
                                return Err(syntax_error("unfinished string", line, start_column));
                            }
                        } else if c == '\n' {
                            return Err(syntax_error("unfinished string", line, start_column));
                        } else {
                            str_value.push(c);
                        }
                    }
                    
                    tokens.push(Token { token_type: TokenType::String(str_value), line, column: start_column });
                },
                
                // Operators
                '+' => {
                    tokens.push(Token { token_type: TokenType::Operator("+".to_string()), line, column });
                    column += 1;
                },
                '-' => {
                    tokens.push(Token { token_type: TokenType::Operator("-".to_string()), line, column });
                    column += 1;
                },
                '*' => {
                    tokens.push(Token { token_type: TokenType::Operator("*".to_string()), line, column });
                    column += 1;
                },
                '/' => {
                    tokens.push(Token { token_type: TokenType::Operator("/".to_string()), line, column });
                    column += 1;
                },
                '%' => {
                    tokens.push(Token { token_type: TokenType::Operator("%".to_string()), line, column });
                    column += 1;
                },
                '^' => {
                    tokens.push(Token { token_type: TokenType::Operator("^".to_string()), line, column });
                    column += 1;
                },
                '=' => {
                    if chars.peek() == Some(&'=') {
                        chars.next();
                        tokens.push(Token { token_type: TokenType::Operator("==".to_string()), line, column });
                        column += 2;
                    } else {
                        tokens.push(Token { token_type: TokenType::Operator("=".to_string()), line, column });
                        column += 1;
                    }
                },
                '~' => {
                    if chars.peek() == Some(&'=') {
                        chars.next();
                        tokens.push(Token { token_type: TokenType::Operator("~=".to_string()), line, column });
                        column += 2;
                    } else {
                        return Err(syntax_error("unexpected symbol '~'", line, column));
                    }
                },
                '<' => {
                    if chars.peek() == Some(&'=') {
                        chars.next();
                        tokens.push(Token { token_type: TokenType::Operator("<=".to_string()), line, column });
                        column += 2;
                    } else {
                        tokens.push(Token { token_type: TokenType::Operator("<".to_string()), line, column });
                        column += 1;
                    }
                },
                '>' => {
                    if chars.peek() == Some(&'=') {
                        chars.next();
                        tokens.push(Token { token_type: TokenType::Operator(">=".to_string()), line, column });
                        column += 2;
                    } else {
                        tokens.push(Token { token_type: TokenType::Operator(">".to_string()), line, column });
                        column += 1;
                    }
                },
                '.' => {
                    if chars.peek() == Some(&'.') {
                        chars.next();
                        if chars.peek() == Some(&'.') {
                            chars.next();
                            tokens.push(Token { token_type: TokenType::Operator("...".to_string()), line, column });
                            column += 3;
                        } else {
                            tokens.push(Token { token_type: TokenType::Operator("..".to_string()), line, column });
                            column += 2;
                        }
                    } else {
                        tokens.push(Token { token_type: TokenType::Separator('.'), line, column });
                        column += 1;
                    }
                },
                
                // Separators
                '(' => {
                    tokens.push(Token { token_type: TokenType::Separator('('), line, column });
                    column += 1;
                },
                ')' => {
                    tokens.push(Token { token_type: TokenType::Separator(')'), line, column });
                    column += 1;
                },
                '[' => {
                    tokens.push(Token { token_type: TokenType::Separator('['), line, column });
                    column += 1;
                },
                ']' => {
                    tokens.push(Token { token_type: TokenType::Separator(']'), line, column });
                    column += 1;
                },
                '{' => {
                    tokens.push(Token { token_type: TokenType::Separator('{'), line, column });
                    column += 1;
                },
                '}' => {
                    tokens.push(Token { token_type: TokenType::Separator('}'), line, column });
                    column += 1;
                },
                ';' => {
                    tokens.push(Token { token_type: TokenType::Separator(';'), line, column });
                    column += 1;
                },
                ',' => {
                    tokens.push(Token { token_type: TokenType::Separator(','), line, column });
                    column += 1;
                },
                
                // Other characters - error
                _ => {
                    return Err(syntax_error(&format!("unexpected symbol '{}'", c), line, column));
                },
            }
        }
        
        // Add EOF token
        tokens.push(Token { token_type: TokenType::Eof, line, column });
        
        Ok(tokens)
    }
    
    /// Parse tokens into an AST
    fn parse_tokens(&mut self, tokens: Vec<Token>) -> Result<Vec<Node<Statement>>> {
        // For now, we'll implement a very simple parser for common Redis Lua syntax
        let mut statements = Vec::new();
        let mut pos = 0;
        
        // Intern string literals
        let mut string_handles = HashMap::new();
        
        while pos < tokens.len() {
            let token = &tokens[pos];
            
            match &token.token_type {
                // Look for "return" statement
                TokenType::Keyword(kw) if kw == "return" => {
                    let line = token.line;
                    let column = token.column;
                    
                    // Move past "return"
                    pos += 1;
                    
                    // Parse expression list
                    let mut exprs = Vec::new();
                    
                    while pos < tokens.len() && !matches!(tokens[pos].token_type, TokenType::Eof | TokenType::Separator(';')) {
                        // Parse expression
                        let (expr, new_pos) = self.parse_expr(&tokens, pos, &mut string_handles)?;
                        exprs.push(expr);
                        pos = new_pos;
                        
                        // Skip comma
                        if pos < tokens.len() && matches!(tokens[pos].token_type, TokenType::Separator(',')) {
                            pos += 1;
                        } else {
                            break;
                        }
                    }
                    
                    // Skip semicolon
                    if pos < tokens.len() && matches!(tokens[pos].token_type, TokenType::Separator(';')) {
                        pos += 1;
                    }
                    
                    // Add return statement
                    statements.push(Node {
                        value: Statement::Return(exprs),
                        line,
                        column,
                    });
                },
                
                // Other statement types not implemented yet
                _ => {
                    // Skip token
                    pos += 1;
                }
            }
        }
        
        // If no statements found, add a default "return nil"
        if statements.is_empty() {
            // Use nil expression
            let nil_expr = Node {
                value: Expression::Nil,
                line: 1,
                column: 8,
            };
            
            // Add return statement
            statements.push(Node {
                value: Statement::Return(vec![nil_expr]),
                line: 1,
                column: 1,
            });
        }
        
        Ok(statements)
    }
    
    /// Parse an expression
    fn parse_expr(&mut self, tokens: &[Token], pos: usize, string_handles: &mut HashMap<String, StringHandle>) -> Result<(Node<Expression>, usize)> {
        if pos >= tokens.len() {
            return Err(syntax_error("unexpected end of input", 1, 1));
        }
        
        let token = &tokens[pos];
        
        match &token.token_type {
            // Literal nil
            TokenType::Keyword(kw) if kw == "nil" => {
                Ok((
                    Node {
                        value: Expression::Nil,
                        line: token.line,
                        column: token.column,
                    },
                    pos + 1,
                ))
            },
            
            // Literal true
            TokenType::Keyword(kw) if kw == "true" => {
                Ok((
                    Node {
                        value: Expression::Boolean(true),
                        line: token.line,
                        column: token.column,
                    },
                    pos + 1,
                ))
            },
            
            // Literal false
            TokenType::Keyword(kw) if kw == "false" => {
                Ok((
                    Node {
                        value: Expression::Boolean(false),
                        line: token.line,
                        column: token.column,
                    },
                    pos + 1,
                ))
            },
            
            // Literal number
            TokenType::Number(n) => {
                Ok((
                    Node {
                        value: Expression::Number(*n),
                        line: token.line,
                        column: token.column,
                    },
                    pos + 1,
                ))
            },
            
            // Literal string
            TokenType::String(s) => {
                // Create a handle for the string
                // This is a simplified approach since we don't have a real string interner here
                let handle = if let Some(&handle) = string_handles.get(s) {
                    handle
                } else {
                    // Create a dummy handle
                    let handle = StringHandle(super::arena::Handle {
                        index: string_handles.len() as u32,
                        generation: 0,
                        _phantom: PhantomData,
                    });
                    string_handles.insert(s.clone(), handle);
                    handle
                };
                
                Ok((
                    Node {
                        value: Expression::String(handle),
                        line: token.line,
                        column: token.column,
                    },
                    pos + 1,
                ))
            },
            
            // Vararg (...)
            TokenType::Operator(op) if op == "..." => {
                Ok((
                    Node {
                        value: Expression::Vararg,
                        line: token.line,
                        column: token.column,
                    },
                    pos + 1,
                ))
            },
            
            // Other types of expressions not implemented yet
            _ => {
                // Return a dummy nil expression
                Ok((
                    Node {
                        value: Expression::Nil,
                        line: token.line,
                        column: token.column,
                    },
                    pos + 1,
                ))
            }
        }
    }
}

/// Parse Lua source code into an AST
pub fn parse(source: &str) -> Result<Vec<Node<Statement>>> {
    let mut parser = Parser::new(source);
    parser.parse()
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_parse_return_nil() {
        let mut parser = Parser::new("return nil");
        let ast = parser.parse().unwrap();
        
        assert_eq!(ast.len(), 1);
        match &ast[0].value {
            Statement::Return(exprs) => {
                assert_eq!(exprs.len(), 1);
                match &exprs[0].value {
                    Expression::Nil => {},
                    _ => panic!("Expected nil expression"),
                }
            },
            _ => panic!("Expected return statement"),
        }
    }
}