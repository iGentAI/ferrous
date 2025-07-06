//! Lua Abstract Syntax Tree
//!
//! This module defines the AST structures used to represent Lua code
//! in a form that can be processed by the bytecode generator.

/// A chunk is a sequence of statements
#[derive(Debug, Clone, PartialEq)]
pub struct Chunk {
    /// The statements in the chunk
    pub statements: Vec<Statement>,
    
    /// Optional return statement
    pub return_statement: Option<ReturnStatement>,
}

/// Statement types
#[derive(Debug, Clone, PartialEq)]
pub enum Statement {
    /// Assignment statement
    Assignment(Assignment),
    
    /// Local variable declaration
    LocalDeclaration(LocalDeclaration),
    
    /// Function call statement
    FunctionCall(FunctionCall),
    
    /// Label definition
    LabelDefinition(String),
    
    /// Break statement
    Break,
    
    /// Goto statement
    Goto(String),
    
    /// Do block
    Do(Block),
    
    /// While loop
    While {
        /// Condition expression
        condition: Expression,
        /// Loop body
        body: Block,
    },
    
    /// Repeat loop
    Repeat {
        /// Loop body
        body: Block,
        /// Until condition
        condition: Expression,
    },
    
    /// If statement
    If {
        /// Main condition
        condition: Expression,
        /// Main body
        body: Block,
        /// Else-if clauses
        else_ifs: Vec<(Expression, Block)>,
        /// Optional else clause
        else_block: Option<Block>,
    },
    
    /// For numeric loop
    ForLoop {
        /// Loop variable name
        variable: String,
        /// Initial value
        initial: Expression,
        /// Limit value
        limit: Expression,
        /// Step value (defaults to 1)
        step: Option<Expression>,
        /// Loop body
        body: Block,
    },
    
    /// For in loop
    ForInLoop {
        /// Loop variable names
        variables: Vec<String>,
        /// Iterator expressions
        iterators: Vec<Expression>,
        /// Loop body
        body: Block,
    },
    
    /// Function definition
    FunctionDefinition {
        /// Function name
        name: FunctionName,
        /// Parameter list
        parameters: Vec<String>,
        /// Is vararg function
        is_vararg: bool,
        /// Function body
        body: Block,
    },
    
    /// Local function definition
    LocalFunctionDefinition {
        /// Function name
        name: String,
        /// Parameter list
        parameters: Vec<String>,
        /// Is vararg function
        is_vararg: bool,
        /// Function body
        body: Block,
    },
    
    /// Return statement
    Return {
        /// Return expressions
        expressions: Vec<Expression>
    },
}

/// A block of statements
#[derive(Debug, Clone, PartialEq)]
pub struct Block {
    /// The statements in the block
    pub statements: Vec<Statement>,
}

/// Assignment statement
#[derive(Debug, Clone, PartialEq)]
pub struct Assignment {
    /// Variables to assign to
    pub variables: Vec<Variable>,
    
    /// Expressions to assign
    pub expressions: Vec<Expression>,
}

/// Local variable declaration
#[derive(Debug, Clone, PartialEq)]
pub struct LocalDeclaration {
    /// Variable names
    pub names: Vec<String>,
    
    /// Initializer expressions (optional)
    pub expressions: Vec<Expression>,
}

/// Variable reference
#[derive(Debug, Clone, PartialEq)]
pub enum Variable {
    /// Simple name
    Name(String),
    
    /// Table indexing: table[key]
    Index {
        /// Table expression
        table: Box<Expression>,
        /// Key expression
        key: Box<Expression>,
    },
    
    /// Table member access: table.key
    Member {
        /// Table expression
        table: Box<Expression>,
        /// Field name
        field: String,
    },
}

/// Expression types
#[derive(Debug, Clone, PartialEq)]
pub enum Expression {
    /// Nil literal
    Nil,
    
    /// Boolean literal
    Boolean(bool),
    
    /// Number literal
    Number(f64),
    
    /// String literal
    String(String),
    
    /// Vararg expression (...)
    VarArg,
    
    /// Function definition
    FunctionDef {
        /// Parameter list
        parameters: Vec<String>,
        /// Is vararg function
        is_vararg: bool,
        /// Function body
        body: Block,
    },
    
    /// Table constructor
    TableConstructor(TableConstructor),
    
    /// Binary operation
    BinaryOp {
        /// Left operand
        left: Box<Expression>,
        /// Operator
        operator: BinaryOperator,
        /// Right operand
        right: Box<Expression>,
    },
    
    /// Unary operation
    UnaryOp {
        /// Operator
        operator: UnaryOperator,
        /// Operand
        operand: Box<Expression>,
    },
    
    /// Variable reference
    Variable(Variable),
    
    /// Function call
    FunctionCall(Box<FunctionCall>),
}

/// Binary operators
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum BinaryOperator {
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
    
    /// Logical and
    And,
    
    /// Logical or
    Or,
}

/// Unary operators
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum UnaryOperator {
    /// Logical not
    Not,
    
    /// Unary minus
    Minus,
    
    /// Length operator (#)
    Length,
}

/// Function call
#[derive(Debug, Clone, PartialEq)]
pub struct FunctionCall {
    /// Function expression
    pub function: Expression,
    
    /// Method name for method calls (obj:method())
    pub method: Option<String>,
    
    /// Arguments
    pub args: CallArgs,
}

/// Call arguments
#[derive(Debug, Clone, PartialEq)]
pub enum CallArgs {
    /// Normal argument list
    Args(Vec<Expression>),
    
    /// Table constructor as single argument
    Table(TableConstructor),
    
    /// String literal as single argument
    String(String),
}

/// Function name
#[derive(Debug, Clone, PartialEq)]
pub struct FunctionName {
    /// The parts of the name (a.b.c)
    pub names: Vec<String>,
    
    /// Optional method part (a.b.c:d)
    pub method: Option<String>,
}

/// Table constructor
#[derive(Debug, Clone, PartialEq)]
pub struct TableConstructor {
    /// Fields in the table
    pub fields: Vec<TableField>,
}

/// Table field
#[derive(Debug, Clone, PartialEq)]
pub enum TableField {
    /// Named field: { name = value }
    Record {
        /// Field name
        key: String,
        /// Field value
        value: Expression,
    },
    
    /// Computed field: { [expr] = value }
    Index {
        /// Field key expression
        key: Expression,
        /// Field value
        value: Expression,
    },
    
    /// List field: { value }
    List(Expression),
}

/// Return statement
#[derive(Debug, Clone, PartialEq)]
pub struct ReturnStatement {
    /// Return expressions
    pub expressions: Vec<Expression>,
}

impl Chunk {
    /// Create a new empty chunk
    pub fn new() -> Self {
        Chunk {
            statements: Vec::new(),
            return_statement: None,
        }
    }
}

impl Block {
    /// Create a new empty block
    pub fn new() -> Self {
        Block {
            statements: Vec::new(),
        }
    }
}

impl TableConstructor {
    /// Create a new empty table constructor
    pub fn new() -> Self {
        TableConstructor {
            fields: Vec::new(),
        }
    }
}

impl Default for Chunk {
    fn default() -> Self {
        Self::new()
    }
}

impl Default for Block {
    fn default() -> Self {
        Self::new()
    }
}

impl Default for TableConstructor {
    fn default() -> Self {
        Self::new()
    }
}