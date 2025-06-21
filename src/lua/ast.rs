//! Abstract Syntax Tree for Lua
//!
//! This module defines the types that represent the structure of Lua code
//! after parsing but before compilation to bytecode.

use crate::lua::value::{LuaString, LuaValue};
use std::fmt;
use std::rc::Rc;

/// A location in the source code (for error reporting)
#[derive(Debug, Clone, Copy)]
pub struct Location {
    /// Line number (1-based)
    pub line: usize,
    
    /// Column number (1-based)
    pub column: usize,
}

impl Location {
    /// Create a new location
    pub fn new(line: usize, column: usize) -> Self {
        Location { line, column }
    }
}

impl fmt::Display for Location {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}:{}", self.line, self.column)
    }
}

/// A complete chunk of Lua code
#[derive(Debug, Clone)]
pub struct Chunk {
    /// The block of statements in the chunk
    pub block: Block,
}

/// A sequence of statements
#[derive(Debug, Clone)]
pub struct Block {
    /// The statements in the block
    pub statements: Vec<Statement>,
    
    /// The optional return statement at the end
    pub return_stmt: Option<ReturnStatement>,
}

/// A Lua statement
#[derive(Debug, Clone)]
pub enum Statement {
    /// An empty statement (semicolon)
    Empty,
    
    /// A variable assignment
    Assignment(AssignmentStatement),
    
    /// A function call
    FunctionCall(FunctionCall),
    
    /// A do...end block
    Do(Box<Block>),
    
    /// A while loop
    While {
        /// The condition
        condition: Expression,
        /// The body
        body: Block,
    },
    
    /// A repeat...until loop
    Repeat {
        /// The body
        body: Block,
        /// The condition
        condition: Expression,
    },
    
    /// An if statement
    If(IfStatement),
    
    /// A numeric for loop
    NumericFor {
        /// The loop variable name
        var: String,
        /// The initial value
        start: Expression,
        /// The end value
        end: Expression,
        /// The step value (optional, defaults to 1)
        step: Option<Expression>,
        /// The body
        body: Block,
    },
    
    /// A generic for loop
    GenericFor {
        /// The loop variable names
        vars: Vec<String>,
        /// The expressions to iterate over
        iterators: Vec<Expression>,
        /// The body
        body: Block,
    },
    
    /// A function definition
    Function(FunctionStatement),
    
    /// A local variable declaration
    LocalAssignment {
        /// The variable names
        names: Vec<String>,
        /// The initial values (optional)
        values: Vec<Expression>,
    },
    
    /// A local function definition
    LocalFunction {
        /// The function name
        name: String,
        /// The function definition
        func: FunctionDefinition,
    },
    
    /// A break statement
    Break,
}

/// A variable assignment statement
#[derive(Debug, Clone)]
pub struct AssignmentStatement {
    /// The variables being assigned to
    pub vars: Vec<Variable>,
    
    /// The values being assigned
    pub values: Vec<Expression>,
}

/// An if statement
#[derive(Debug, Clone)]
pub struct IfStatement {
    /// The condition for the main if
    pub condition: Expression,
    
    /// The body for the main if
    pub then_block: Block,
    
    /// The elseif branches (condition and body)
    pub elseif_branches: Vec<(Expression, Block)>,
    
    /// The optional else branch
    pub else_block: Option<Block>,
}

/// A function statement (global or method)
#[derive(Debug, Clone)]
pub struct FunctionStatement {
    /// The function name
    pub name: FunctionName,
    
    /// The function definition
    pub func: FunctionDefinition,
}

/// A function name which can include a table and/or method
#[derive(Debug, Clone)]
pub struct FunctionName {
    /// The base name
    pub base: String,
    
    /// The field access chain (if any)
    pub fields: Vec<String>,
    
    /// The method name (if any)
    pub method: Option<String>,
}

/// A function definition
#[derive(Debug, Clone)]
pub struct FunctionDefinition {
    /// The parameter names
    pub parameters: Vec<String>,
    
    /// Whether the function is variadic
    pub is_variadic: bool,
    
    /// The function body
    pub body: Block,
}

/// A return statement
#[derive(Debug, Clone)]
pub struct ReturnStatement {
    /// The values to return
    pub values: Vec<Expression>,
}

/// A variable reference (name, table access, etc.)
#[derive(Debug, Clone)]
pub enum Variable {
    /// A simple variable name
    Name(String),
    
    /// A table field access
    Field {
        /// The table expression
        table: Box<Expression>,
        /// The field name
        key: Box<Expression>,
    },
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
    /// Exponentiation (^)
    Pow,
    /// Concatenation (..)
    Concat,
    /// Less than (<)
    Less,
    /// Less than or equal (<=)
    LessEqual,
    /// Greater than (>)
    Greater,
    /// Greater than or equal (>=)
    GreaterEqual,
    /// Equality (==)
    Eq,
    /// Inequality (~=)
    NotEqual,
    /// Logical and
    And,
    /// Logical or
    Or,
}

/// A unary operator
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum UnaryOp {
    /// Arithmetic negation (-)
    Neg,
    /// Logical not
    Not,
    /// Length operator (#)
    Len,
}

/// A Lua expression
#[derive(Debug, Clone)]
pub enum Expression {
    /// A nil literal
    Nil,
    
    /// A boolean literal
    Boolean(bool),
    
    /// A number literal
    Number(f64),
    
    /// A string literal
    String(LuaString),
    
    /// A variable reference
    Variable(Variable),
    
    /// A function call
    FunctionCall(FunctionCall),
    
    /// A binary operation
    BinaryOp {
        /// The operator
        op: BinaryOp,
        /// The left operand
        left: Box<Expression>,
        /// The right operand
        right: Box<Expression>,
    },
    
    /// A unary operation
    UnaryOp {
        /// The operator
        op: UnaryOp,
        /// The operand
        operand: Box<Expression>,
    },
    
    /// A function definition
    Function(FunctionDefinition),
    
    /// A table constructor
    Table(Vec<TableField>),
    
    /// Vararg expression (...)
    Vararg,
}

/// A function call
#[derive(Debug, Clone)]
pub struct FunctionCall {
    /// The function expression
    pub func: Box<Expression>,
    
    /// The arguments to the function
    pub args: Vec<Expression>,
    
    /// Whether this is a method call with a colon (obj:method())
    pub is_method_call: bool,
    
    /// The method name if this is a method call
    pub method_name: Option<String>,
}

/// A field in a table constructor
#[derive(Debug, Clone)]
pub enum TableField {
    /// A simple value (implicit key)
    Value(Expression),
    
    /// A key-value pair
    KeyValue {
        /// The key
        key: Expression,
        /// The value
        value: Expression,
    },
    
    /// A field with a name key
    NamedField {
        /// The field name
        name: String,
        /// The value
        value: Expression,
    },
}

/// Convert a Lua value to an expression
impl From<LuaValue> for Expression {
    fn from(value: LuaValue) -> Self {
        match value {
            LuaValue::Nil => Expression::Nil,
            LuaValue::Boolean(b) => Expression::Boolean(b),
            LuaValue::Number(n) => Expression::Number(n),
            LuaValue::String(s) => Expression::String(s),
            _ => Expression::Nil, // Other types can't be converted directly
        }
    }
}