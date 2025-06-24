//! Abstract Syntax Tree (AST) for Lua 5.1

use crate::lua_new::value::StringHandle;

/// A location in the source code
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SourceLoc {
    pub line: u16,
    pub column: u16,
}

/// A node in the AST with location information
#[derive(Debug, Clone)]
pub struct Node<T> {
    pub node: T,
    pub loc: SourceLoc,
}

impl<T> Node<T> {
    pub fn new(node: T, loc: SourceLoc) -> Self {
        Node { node, loc }
    }
}

impl<T: std::fmt::Debug> Node<T> {
    /// Pretty print the node for debugging 
    pub fn pretty_print(&self, indent: usize) -> String {
        let indent_str = " ".repeat(indent);
        format!("{}{:?} at line: {}, col: {}", 
                indent_str, 
                self.node, 
                self.loc.line, 
                self.loc.column)
    }
}

/// A chunk (sequence of statements with optional return)
#[derive(Debug, Clone)]
pub struct Chunk {
    pub statements: Vec<Node<Statement>>,
    pub ret: Option<Node<ReturnStatement>>,
}

/// A statement in Lua
#[derive(Debug, Clone)]
pub enum Statement {
    /// Empty statement (just a semicolon)
    Empty,
    
    /// Assignment: var1, var2, ... = exp1, exp2, ...
    Assignment(Assignment),
    
    /// Local assignment: local var1, var2, ... = exp1, exp2, ...
    LocalAssignment(LocalAssignment),
    
    /// Function call as statement
    FunctionCall(FunctionCall),
    
    /// Function definition: function name(...) ... end
    FunctionDefinition(FunctionDefinition),
    
    /// Local function: local function name(...) ... end
    LocalFunction(FunctionDefinition),
    
    /// Do block: do ... end
    DoBlock(Chunk),
    
    /// While loop: while exp do ... end
    WhileLoop {
        condition: Node<Expression>,
        body: Chunk,
    },
    
    /// Repeat loop: repeat ... until exp
    RepeatLoop {
        body: Chunk,
        condition: Node<Expression>,
    },
    
    /// If statement: if exp then ... elseif exp then ... else ... end
    IfStatement {
        clauses: Vec<(Node<Expression>, Chunk)>,
        else_clause: Option<Chunk>,
    },
    
    /// For loop (numeric): for var=start,limit,step do ... end
    NumericFor {
        variable: StringHandle,
        start: Node<Expression>,
        limit: Node<Expression>,
        step: Option<Node<Expression>>,
        body: Chunk,
    },
    
    /// For loop (generic): for var1,var2,... in exp1,exp2,... do ... end
    GenericFor {
        variables: Vec<StringHandle>,
        iterators: Vec<Node<Expression>>,
        body: Chunk,
    },
    
    /// Break statement
    Break,
}

/// An assignment statement
#[derive(Debug, Clone)]
pub struct Assignment {
    pub variables: Vec<Node<Variable>>,
    pub expressions: Vec<Node<Expression>>,
}

/// A local assignment statement
#[derive(Debug, Clone)]
pub struct LocalAssignment {
    pub names: Vec<StringHandle>,
    pub expressions: Vec<Node<Expression>>,
}

/// A return statement
#[derive(Debug, Clone)]
pub struct ReturnStatement {
    pub expressions: Vec<Node<Expression>>,
}

/// A function definition
#[derive(Debug, Clone)]
pub struct FunctionDefinition {
    pub name: FunctionName,
    pub parameters: FunctionParameters,
    pub body: Chunk,
}

/// Function name (can be simple, table field, or method)
#[derive(Debug, Clone)]
pub enum FunctionName {
    /// Simple name: function foo()
    Simple(StringHandle),
    
    /// Table field: function a.b.c()
    TableField {
        base: StringHandle,
        fields: Vec<StringHandle>,
    },
    
    /// Method: function a.b:method()
    Method {
        base: StringHandle,
        fields: Vec<StringHandle>,
        method: StringHandle,
    },
}

/// Function parameters
#[derive(Debug, Clone)]
pub struct FunctionParameters {
    pub names: Vec<StringHandle>,
    pub is_variadic: bool,
}

/// An expression in Lua
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
    
    /// Variable reference
    Variable(Variable),
    
    /// Vararg expression (...)
    Vararg,
    
    /// Function call
    FunctionCall(FunctionCall),
    
    /// Table constructor
    TableConstructor(TableConstructor),
    
    /// Anonymous function
    AnonymousFunction {
        parameters: FunctionParameters,
        body: Chunk,
    },
    
    /// Binary operation
    BinaryOp {
        op: BinaryOperator,
        left: Box<Node<Expression>>,
        right: Box<Node<Expression>>,
    },
    
    /// Unary operation
    UnaryOp {
        op: UnaryOperator,
        operand: Box<Node<Expression>>,
    },
}

/// A variable (can be simple, table field, or table index)
#[derive(Debug, Clone)]
pub enum Variable {
    /// Simple variable: foo
    Name(StringHandle),
    
    /// Table field: expr[expr]
    TableField {
        table: Box<Node<Expression>>,
        key: Box<Node<Expression>>,
    },
    
    /// Table dot access: expr.name
    TableDot {
        table: Box<Node<Expression>>,
        key: StringHandle,
    },
}

/// A function call
#[derive(Debug, Clone)]
pub struct FunctionCall {
    pub function: Box<Node<Expression>>,
    pub arguments: Vec<Node<Expression>>,
    pub is_method_call: bool,
    pub method_name: Option<StringHandle>,
}

/// A table constructor
#[derive(Debug, Clone)]
pub struct TableConstructor {
    pub fields: Vec<TableField>,
}

/// A field in a table constructor
#[derive(Debug, Clone)]
pub enum TableField {
    /// Array part: expr
    Array(Node<Expression>),
    
    /// Record part: name = expr
    Record {
        key: StringHandle,
        value: Node<Expression>,
    },
    
    /// General field: [expr] = expr
    Expression {
        key: Node<Expression>,
        value: Node<Expression>,
    },
}

/// Binary operators
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BinaryOperator {
    Add,    // +
    Sub,    // -
    Mul,    // *
    Div,    // /
    Mod,    // %
    Pow,    // ^
    Concat, // ..
    LT,     // <
    LE,     // <=
    GT,     // >
    GE,     // >=
    EQ,     // ==
    NE,     // ~=
    And,    // and
    Or,     // or
}

/// Unary operators
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UnaryOperator {
    Minus, // -
    Not,   // not
    Len,   // #
}