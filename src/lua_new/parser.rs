//! Parser for Lua scripts
//!
//! This module provides a recursive descent parser for Lua 5.1,
//! which converts tokens into an abstract syntax tree (AST).

use crate::lua_new::error::{LuaError, Result};
use crate::lua_new::value::{StringHandle};
use crate::lua_new::lexer::{Lexer, Token, TokenType};
use crate::lua_new::ast::{
    Chunk, Statement, Expression, Variable, FunctionCall, TableConstructor,
    TableField, ReturnStatement, Assignment, LocalAssignment, FunctionDefinition,
    FunctionName, FunctionParameters, BinaryOperator, UnaryOperator, Node, SourceLoc
};

/// The Lua parser
pub struct Parser<'a> {
    /// The lexer to get tokens from
    lexer: Lexer<'a>,
    
    /// Current token
    current_token: Option<Token>,
    
    /// Heap for creating string handles
    heap: &'a mut crate::lua_new::heap::LuaHeap,
    
    /// Source code (for error reporting)
    source: &'a str,
}

impl<'a> Parser<'a> {
    /// Create a new parser
    pub fn new(source: &'a str, heap: &'a mut crate::lua_new::heap::LuaHeap) -> Result<Self> {
        let mut lexer = Lexer::new(source);
        let current_token = Some(lexer.next_token()?);
        
        Ok(Parser {
            lexer,
            current_token,
            heap,
            source,
        })
    }
    
    /// Get the current token
    fn current_token(&self) -> Result<&Token> {
        self.current_token.as_ref().ok_or_else(|| LuaError::InvalidOperation("No current token".to_string()))
    }
    
    /// Advance to the next token
    fn advance(&mut self) -> Result<()> {
        self.current_token = Some(self.lexer.next_token()?);
        Ok(())
    }
    
    /// Check if current token is of the given type
    fn check(&self, token_type: &TokenType) -> bool {
        if let Ok(token) = self.current_token() {
            matches!(&token.token_type, t if t == token_type)
        } else {
            false
        }
    }
    
    /// Consume the current token if it matches the expected type
    fn consume(&mut self, token_type: &TokenType) -> Result<Token> {
        let token = self.current_token()?.clone();
        
        if &token.token_type == token_type {
            self.advance()?;
            Ok(token)
        } else {
            Err(LuaError::SyntaxError {
                message: format!("Expected {:?}, found {:?}", token_type, token.token_type),
                line: token.line as usize,
                column: token.column as usize,
            })
        }
    }
    
    /// Create a string handle for an identifier
    fn create_string_handle(&mut self, s: &str) -> StringHandle {
        self.heap.create_string(s)
    }
    
    /// Parse a Lua script and return the AST
    pub fn parse(&mut self) -> Result<Chunk> {
        self.parse_chunk()
    }
    
    /// Parse a chunk (sequence of statements with optional return)
    fn parse_chunk(&mut self) -> Result<Chunk> {
        let mut statements = Vec::new();
        let mut ret = None;
        
        while !self.check(&TokenType::EOF) && !self.is_block_end() {
            // Check for return statement (must be last statement in chunk)
            if self.check(&TokenType::Return) {
                ret = Some(self.parse_return_statement()?);
                
                // Optional semicolon after return
                if self.check(&TokenType::Semicolon) {
                    self.advance()?;
                }
                
                break;
            }
            
            // Parse regular statement
            let stmt = self.parse_statement()?;
            statements.push(stmt);
            
            // Optional semicolon between statements
            if self.check(&TokenType::Semicolon) {
                self.advance()?;
            }
        }
        
        Ok(Chunk { statements, ret })
    }
    
    /// Check if the current token indicates end of a block
    fn is_block_end(&self) -> bool {
        if let Ok(token) = self.current_token() {
            matches!(token.token_type, 
                     TokenType::End | 
                     TokenType::Else | 
                     TokenType::Elseif | 
                     TokenType::Until)
        } else {
            true
        }
    }
    
    /// Parse a statement
    fn parse_statement(&mut self) -> Result<Node<Statement>> {
        let token = self.current_token()?;
        let loc = SourceLoc { line: token.line, column: token.column };
        
        match &token.token_type {
            TokenType::Semicolon => {
                self.advance()?;
                Ok(Node::new(Statement::Empty, loc))
            },
            
            TokenType::Local => {
                self.advance()?;
                
                if self.check(&TokenType::Function) {
                    self.parse_local_function_definition(loc)
                } else {
                    self.parse_local_assignment(loc)
                }
            },
            
            TokenType::Function => {
                self.advance()?;
                self.parse_function_definition(loc, false)
            },
            
            TokenType::Do => {
                self.advance()?;
                let body = self.parse_chunk()?;
                self.consume(&TokenType::End)?;
                
                Ok(Node::new(Statement::DoBlock(body), loc))
            },
            
            TokenType::While => {
                self.advance()?;
                let condition = self.parse_expression()?;
                self.consume(&TokenType::Do)?;
                
                let body = self.parse_chunk()?;
                self.consume(&TokenType::End)?;
                
                Ok(Node::new(Statement::WhileLoop { condition, body }, loc))
            },
            
            TokenType::If => {
                self.parse_if_statement()
            },
            
            TokenType::For => {
                self.parse_for_statement()
            },
            
            TokenType::Repeat => {
                self.advance()?;
                let body = self.parse_chunk()?;
                self.consume(&TokenType::Until)?;
                
                let condition = self.parse_expression()?;
                
                Ok(Node::new(Statement::RepeatLoop { body, condition }, loc))
            },
            
            TokenType::Break => {
                self.advance()?;
                Ok(Node::new(Statement::Break, loc))
            },
            
            // Function call or assignment
            _ => {
                // Try to parse a variable list (potential assignment)
                let var = match self.try_parse_prefixexp()? {
                    Some(expr) => expr,
                    None => {
                        // Get the current token for error reporting
                        let token = self.current_token()?;
                        return Err(LuaError::SyntaxError {
                            message: "Expected statement".to_string(),
                            line: token.line as usize,
                            column: token.column as usize,
                        });
                    }
                };
                
                // Check what kind of expression/statement this is
                match &var.node {
                    Expression::FunctionCall(call) => {
                        // This is a function call statement
                        Ok(Node::new(Statement::FunctionCall(call.clone()), loc))
                    },
                    Expression::Variable(var_node) => {
                        // This could be the start of an assignment
                        let mut variables = vec![Node::new(var_node.clone(), loc)];
                        
                        // Check for more variables in assignment
                        while self.check(&TokenType::Comma) {
                            self.advance()?;
                            
                            // Get the next token directly before attempting to parse another variable
                            let next_token = self.current_token()?.clone();
                            let next_expr = match self.try_parse_prefixexp()? {
                                Some(expr) => expr,
                                None => {
                                    return Err(LuaError::SyntaxError {
                                        message: "Expected variable after comma".to_string(),
                                        line: next_token.line as usize,
                                        column: next_token.column as usize,
                                    });
                                }
                            };
                            
                            // Extract variable from expression
                            match &next_expr.node {
                                Expression::Variable(var_node) => {
                                    variables.push(Node::new(var_node.clone(), next_expr.loc));
                                },
                                _ => {
                                    return Err(LuaError::SyntaxError {
                                        message: "Expected variable in assignment".to_string(),
                                        line: next_expr.loc.line as usize,
                                        column: next_expr.loc.column as usize,
                                    });
                                }
                            }
                        }
                        
                        // Must be an assignment if we have an equals sign
                        if self.check(&TokenType::Assign) {
                            self.advance()?;
                            let expressions = self.parse_expression_list()?;
                            
                            Ok(Node::new(Statement::Assignment(Assignment { variables, expressions }), loc))
                        } else {
                            // Single variable without assignment - must be a function call
                            return Err(LuaError::SyntaxError {
                                message: "Expected '=' in assignment".to_string(),
                                line: self.current_token()?.line as usize,
                                column: self.current_token()?.column as usize,
                            });
                        }
                    },
                    _ => {
                        return Err(LuaError::SyntaxError {
                            message: "Invalid statement".to_string(),
                            line: var.loc.line as usize,
                            column: var.loc.column as usize,
                        });
                    }
                }
            }
        }
    }
    
    /// Parse a local function definition
    fn parse_local_function_definition(&mut self, loc: SourceLoc) -> Result<Node<Statement>> {
        self.consume(&TokenType::Function)?;
        
        // Function name (must be a simple identifier for local functions)
        let name_token = self.current_token()?;
        let name = if let TokenType::Identifier(id) = &name_token.token_type.clone() {
            self.advance()?;
            FunctionName::Simple(self.create_string_handle(id))
        } else {
            return Err(LuaError::SyntaxError {
                message: "Expected identifier after 'local function'".to_string(),
                line: name_token.line as usize,
                column: name_token.column as usize,
            });
        };
        
        // Function parameters and body
        let (parameters, body) = self.parse_function_body()?;
        
        let func_def = FunctionDefinition { name, parameters, body };
        Ok(Node::new(Statement::LocalFunction(func_def), loc))
    }
    
    /// Parse a local variable assignment
    fn parse_local_assignment(&mut self, loc: SourceLoc) -> Result<Node<Statement>> {
        let mut names = Vec::new();
        
        // Parse variable names
        if let TokenType::Identifier(id) = &self.current_token()?.token_type.clone() {
            names.push(self.create_string_handle(id));
            self.advance()?;
            
            // Parse additional names
            while self.check(&TokenType::Comma) {
                self.advance()?;
                
                if let TokenType::Identifier(id) = &self.current_token()?.token_type.clone() {
                    names.push(self.create_string_handle(id));
                    self.advance()?;
                } else {
                    return Err(LuaError::SyntaxError {
                        message: "Expected identifier after ','".to_string(),
                        line: self.current_token()?.line as usize,
                        column: self.current_token()?.column as usize,
                    });
                }
            }
        }
        
        // Parse expressions (optional)
        let expressions = if self.check(&TokenType::Assign) {
            self.advance()?; // Skip '='
            self.parse_expression_list()?
        } else {
            Vec::new()
        };
        
        Ok(Node::new(
            Statement::LocalAssignment(LocalAssignment { names, expressions }),
            loc
        ))
    }
    
    /// Parse a function definition
    fn parse_function_definition(&mut self, loc: SourceLoc, is_local: bool) -> Result<Node<Statement>> {
        // Parse function name
        let name = self.parse_function_name()?;
        
        // Parse function parameters and body
        let (parameters, body) = self.parse_function_body()?;
        
        let func_def = FunctionDefinition { name, parameters, body };
        
        if is_local {
            Ok(Node::new(Statement::LocalFunction(func_def), loc))
        } else {
            Ok(Node::new(Statement::FunctionDefinition(func_def), loc))
        }
    }
    
    /// Parse a function name (may include table/method components)
    fn parse_function_name(&mut self) -> Result<FunctionName> {
        let name_token = self.current_token()?;
        
        if let TokenType::Identifier(id) = &name_token.token_type.clone() {
            let base = self.create_string_handle(id);
            self.advance()?;
            
            let mut fields = Vec::new();
            
            // Parse dot-separated name parts (field access)
            while self.check(&TokenType::Dot) {
                self.advance()?;
                
                if let TokenType::Identifier(id) = &self.current_token()?.token_type.clone() {
                    fields.push(self.create_string_handle(id));
                    self.advance()?;
                } else {
                    return Err(LuaError::SyntaxError {
                        message: "Expected identifier after '.'".to_string(),
                        line: self.current_token()?.line as usize,
                        column: self.current_token()?.column as usize,
                    });
                }
            }
            
            // Check for method syntax (colon)
            if self.check(&TokenType::Colon) {
                self.advance()?;
                
                if let TokenType::Identifier(id) = &self.current_token()?.token_type.clone() {
                    let method = self.create_string_handle(id);
                    self.advance()?;
                    
                    Ok(FunctionName::Method { base, fields, method })
                } else {
                    Err(LuaError::SyntaxError {
                        message: "Expected identifier after ':'".to_string(),
                        line: self.current_token()?.line as usize,
                        column: self.current_token()?.column as usize,
                    })
                }
            } else if fields.is_empty() {
                Ok(FunctionName::Simple(base))
            } else {
                Ok(FunctionName::TableField { base, fields })
            }
        } else {
            Err(LuaError::SyntaxError {
                message: "Expected identifier for function name".to_string(),
                line: name_token.line as usize,
                column: name_token.column as usize,
            })
        }
    }
    
    /// Parse function parameters and body
    fn parse_function_body(&mut self) -> Result<(FunctionParameters, Chunk)> {
        // Parse parameters
        self.consume(&TokenType::LeftParen)?;
        let parameters = self.parse_parameter_list()?;
        self.consume(&TokenType::RightParen)?;
        
        // Parse function body
        let body = self.parse_chunk()?;
        
        // Consume 'end'
        self.consume(&TokenType::End)?;
        
        Ok((parameters, body))
    }
    
    /// Parse a parameter list for a function
    fn parse_parameter_list(&mut self) -> Result<FunctionParameters> {
        let mut names = Vec::new();
        let mut is_variadic = false;
        
        // Empty parameter list
        if self.check(&TokenType::RightParen) {
            return Ok(FunctionParameters { names, is_variadic });
        }
        
        // First parameter
        if self.check(&TokenType::Vararg) {
            // Function(...) - variadic with no named parameters
            is_variadic = true;
            self.advance()?;
        } else {
            // Named parameter
            if let TokenType::Identifier(id) = &self.current_token()?.token_type.clone() {
                names.push(self.create_string_handle(id));
                self.advance()?;
                
                // Additional parameters
                while self.check(&TokenType::Comma) {
                    self.advance()?;
                    
                    if self.check(&TokenType::Vararg) {
                        // Last parameter is ...
                        is_variadic = true;
                        self.advance()?;
                        break;
                    }
                    
                    if let TokenType::Identifier(id) = &self.current_token()?.token_type.clone() {
                        names.push(self.create_string_handle(id));
                        self.advance()?;
                    } else {
                        return Err(LuaError::SyntaxError {
                            message: "Expected identifier or '...' after ','".to_string(),
                            line: self.current_token()?.line as usize,
                            column: self.current_token()?.column as usize,
                        });
                    }
                }
            } else {
                return Err(LuaError::SyntaxError {
                    message: "Expected identifier or '...'".to_string(),
                    line: self.current_token()?.line as usize,
                    column: self.current_token()?.column as usize,
                });
            }
        }
        
        Ok(FunctionParameters { names, is_variadic })
    }
    
    /// Parse an if statement
    fn parse_if_statement(&mut self) -> Result<Node<Statement>> {
        let token = self.current_token()?;
        let loc = SourceLoc { line: token.line, column: token.column };
        
        self.consume(&TokenType::If)?;
        
        let mut clauses = Vec::new();
        
        // Parse the initial "if condition then" clause
        let condition = self.parse_expression()?;
        self.consume(&TokenType::Then)?;
        let body = self.parse_chunk()?;
        clauses.push((condition, body));
        
        // Parse any "elseif condition then" clauses
        while self.check(&TokenType::Elseif) {
            self.advance()?;
            let condition = self.parse_expression()?;
            self.consume(&TokenType::Then)?;
            let body = self.parse_chunk()?;
            clauses.push((condition, body));
        }
        
        // Parse the optional "else" clause
        let else_clause = if self.check(&TokenType::Else) {
            self.advance()?;
            Some(self.parse_chunk()?)
        } else {
            None
        };
        
        // Consume the final "end"
        self.consume(&TokenType::End)?;
        
        Ok(Node::new(Statement::IfStatement { clauses, else_clause }, loc))
    }
    
    /// Parse a for statement (can be numeric or generic)
    fn parse_for_statement(&mut self) -> Result<Node<Statement>> {
        let token_clone = self.current_token()?.clone();
        let loc = SourceLoc { line: token_clone.line, column: token_clone.column };
        
        self.consume(&TokenType::For)?;
        
        // Check for the variable name
        let first_var = if let TokenType::Identifier(id) = &self.current_token()?.token_type.clone() {
            let var_name = self.create_string_handle(id);
            self.advance()?;
            var_name
        } else {
            return Err(LuaError::SyntaxError {
                message: "Expected identifier after 'for'".to_string(),
                line: token_clone.line as usize,
                column: token_clone.column as usize,
            });
        };
        
        // Decide if this is a numeric for or generic for
        if self.check(&TokenType::Assign) { // Numeric for: for i=1,10,2 do
            self.advance()?; // Skip '='
            
            // Parse start, limit, and step expressions
            let start = self.parse_expression()?;
            self.consume(&TokenType::Comma)?;
            
            let limit = self.parse_expression()?;
            
            // Optional step expression
            let step = if self.check(&TokenType::Comma) {
                self.advance()?;
                Some(self.parse_expression()?)
            } else {
                None
            };
            
            self.consume(&TokenType::Do)?;
            
            let body = self.parse_chunk()?;
            self.consume(&TokenType::End)?;
            
            Ok(Node::new(Statement::NumericFor { variable: first_var, start, limit, step, body }, loc))
        } else { // Generic for: for k,v in pairs(t) do
            let mut variables = vec![first_var];
            
            // Parse additional variables
            while self.check(&TokenType::Comma) {
                self.advance()?;
                
                if let TokenType::Identifier(id) = &self.current_token()?.token_type.clone() {
                    let var_name = self.create_string_handle(id);
                    self.advance()?;
                    variables.push(var_name);
                } else {
                    return Err(LuaError::SyntaxError {
                        message: "Expected identifier after ','".to_string(),
                        line: self.current_token()?.line as usize,
                        column: self.current_token()?.column as usize,
                    });
                }
            }
            
            self.consume(&TokenType::In)?;
            
            // Parse iterator expressions
            let iterators = self.parse_expression_list()?;
            
            self.consume(&TokenType::Do)?;
            
            let body = self.parse_chunk()?;
            self.consume(&TokenType::End)?;
            
            Ok(Node::new(Statement::GenericFor { variables, iterators, body }, loc))
        }
    }
    
    /// Parse a return statement
    fn parse_return_statement(&mut self) -> Result<Node<ReturnStatement>> {
        let token = self.current_token()?;
        let loc = SourceLoc { line: token.line, column: token.column };
        
        self.consume(&TokenType::Return)?;
        
        let expressions = if self.is_block_end() || self.check(&TokenType::Semicolon) {
            Vec::new()
        } else {
            self.parse_expression_list()?
        };
        
        Ok(Node::new(ReturnStatement { expressions }, loc))
    }
    
    /// Parse a list of expressions
    fn parse_expression_list(&mut self) -> Result<Vec<Node<Expression>>> {
        let mut expressions = Vec::new();
        
        expressions.push(self.parse_expression()?);
        
        while self.check(&TokenType::Comma) {
            self.advance()?;
            expressions.push(self.parse_expression()?);
        }
        
        Ok(expressions)
    }
    
    /// Parse a single expression
    fn parse_expression(&mut self) -> Result<Node<Expression>> {
        self.parse_or_expr()
    }
    
    /// Parse logical or expression
    fn parse_or_expr(&mut self) -> Result<Node<Expression>> {
        let mut expr = self.parse_and_expr()?;
        
        while self.check(&TokenType::Or) {
            let token = self.current_token()?;
            let loc = SourceLoc { line: token.line, column: token.column };
            
            self.advance()?;
            let right = self.parse_and_expr()?;
            
            expr = Node::new(
                Expression::BinaryOp {
                    op: BinaryOperator::Or,
                    left: Box::new(expr),
                    right: Box::new(right),
                },
                loc,
            );
        }
        
        Ok(expr)
    }
    
    /// Parse logical and expression
    fn parse_and_expr(&mut self) -> Result<Node<Expression>> {
        let mut expr = self.parse_comparison_expr()?;
        
        while self.check(&TokenType::And) {
            let token = self.current_token()?;
            let loc = SourceLoc { line: token.line, column: token.column };
            
            self.advance()?;
            let right = self.parse_comparison_expr()?;
            
            expr = Node::new(
                Expression::BinaryOp {
                    op: BinaryOperator::And,
                    left: Box::new(expr),
                    right: Box::new(right),
                },
                loc,
            );
        }
        
        Ok(expr)
    }
    
    /// Parse comparison expression
    fn parse_comparison_expr(&mut self) -> Result<Node<Expression>> {
        let mut expr = self.parse_concat_expr()?;
        
        // Handle comparison operators
        if let Ok(token) = self.current_token() {
            let loc = SourceLoc { line: token.line, column: token.column };
            
            match &token.token_type {
                TokenType::Equal => {
                    self.advance()?;
                    let right = self.parse_concat_expr()?;
                    
                    expr = Node::new(
                        Expression::BinaryOp {
                            op: BinaryOperator::EQ,
                            left: Box::new(expr),
                            right: Box::new(right),
                        },
                        loc,
                    );
                },
                TokenType::NotEqual => {
                    self.advance()?;
                    let right = self.parse_concat_expr()?;
                    
                    expr = Node::new(
                        Expression::BinaryOp {
                            op: BinaryOperator::NE,
                            left: Box::new(expr),
                            right: Box::new(right),
                        },
                        loc,
                    );
                },
                TokenType::Less => {
                    self.advance()?;
                    let right = self.parse_concat_expr()?;
                    
                    expr = Node::new(
                        Expression::BinaryOp {
                            op: BinaryOperator::LT,
                            left: Box::new(expr),
                            right: Box::new(right),
                        },
                        loc,
                    );
                },
                TokenType::LessEqual => {
                    self.advance()?;
                    let right = self.parse_concat_expr()?;
                    
                    expr = Node::new(
                        Expression::BinaryOp {
                            op: BinaryOperator::LE,
                            left: Box::new(expr),
                            right: Box::new(right),
                        },
                        loc,
                    );
                },
                TokenType::Greater => {
                    self.advance()?;
                    let right = self.parse_concat_expr()?;
                    
                    expr = Node::new(
                        Expression::BinaryOp {
                            op: BinaryOperator::GT,
                            left: Box::new(expr),
                            right: Box::new(right),
                        },
                        loc,
                    );
                },
                TokenType::GreaterEqual => {
                    self.advance()?;
                    let right = self.parse_concat_expr()?;
                    
                    expr = Node::new(
                        Expression::BinaryOp {
                            op: BinaryOperator::GE,
                            left: Box::new(expr),
                            right: Box::new(right),
                        },
                        loc,
                    );
                },
                _ => {},
            }
        }
        
        Ok(expr)
    }
    
    /// Parse concatenation expression
    fn parse_concat_expr(&mut self) -> Result<Node<Expression>> {
        let mut expr = self.parse_additive_expr()?;
        
        while self.check(&TokenType::Concat) {
            let token = self.current_token()?;
            let loc = SourceLoc { line: token.line, column: token.column };
            
            self.advance()?;
            let right = self.parse_additive_expr()?;
            
            expr = Node::new(
                Expression::BinaryOp {
                    op: BinaryOperator::Concat,
                    left: Box::new(expr),
                    right: Box::new(right),
                },
                loc,
            );
        }
        
        Ok(expr)
    }
    
    /// Parse additive expression (+ and -)
    fn parse_additive_expr(&mut self) -> Result<Node<Expression>> {
        let mut expr = self.parse_multiplicative_expr()?;
        
        loop {
            if let Ok(token) = self.current_token() {
                let loc = SourceLoc { line: token.line, column: token.column };
                
                match &token.token_type {
                    TokenType::Plus => {
                        self.advance()?;
                        let right = self.parse_multiplicative_expr()?;
                        
                        expr = Node::new(
                            Expression::BinaryOp {
                                op: BinaryOperator::Add,
                                left: Box::new(expr),
                                right: Box::new(right),
                            },
                            loc,
                        );
                    },
                    TokenType::Minus => {
                        self.advance()?;
                        let right = self.parse_multiplicative_expr()?;
                        
                        expr = Node::new(
                            Expression::BinaryOp {
                                op: BinaryOperator::Sub,
                                left: Box::new(expr),
                                right: Box::new(right),
                            },
                            loc,
                        );
                    },
                    _ => break,
                }
            } else {
                break;
            }
        }
        
        Ok(expr)
    }
    
    /// Parse multiplicative expression (*, /, %)
    fn parse_multiplicative_expr(&mut self) -> Result<Node<Expression>> {
        let mut expr = self.parse_unary_expr()?;
        
        loop {
            if let Ok(token) = self.current_token() {
                let loc = SourceLoc { line: token.line, column: token.column };
                
                match &token.token_type {
                    TokenType::Multiply => {
                        self.advance()?;
                        let right = self.parse_unary_expr()?;
                        
                        expr = Node::new(
                            Expression::BinaryOp {
                                op: BinaryOperator::Mul,
                                left: Box::new(expr),
                                right: Box::new(right),
                            },
                            loc,
                        );
                    },
                    TokenType::Divide => {
                        self.advance()?;
                        let right = self.parse_unary_expr()?;
                        
                        expr = Node::new(
                            Expression::BinaryOp {
                                op: BinaryOperator::Div,
                                left: Box::new(expr),
                                right: Box::new(right),
                            },
                            loc,
                        );
                    },
                    TokenType::Modulo => {
                        self.advance()?;
                        let right = self.parse_unary_expr()?;
                        
                        expr = Node::new(
                            Expression::BinaryOp {
                                op: BinaryOperator::Mod,
                                left: Box::new(expr),
                                right: Box::new(right),
                            },
                            loc,
                        );
                    },
                    _ => break,
                }
            } else {
                break;
            }
        }
        
        Ok(expr)
    }
    
    /// Parse unary expression (-, not, #)
    fn parse_unary_expr(&mut self) -> Result<Node<Expression>> {
        if let Ok(token) = self.current_token() {
            let loc = SourceLoc { line: token.line, column: token.column };
            
            match &token.token_type {
                TokenType::Minus => {
                    self.advance()?;
                    let operand = self.parse_unary_expr()?;
                    
                    return Ok(Node::new(
                        Expression::UnaryOp {
                            op: UnaryOperator::Minus,
                            operand: Box::new(operand),
                        },
                        loc,
                    ));
                },
                TokenType::Not => {
                    self.advance()?;
                    let operand = self.parse_unary_expr()?;
                    
                    return Ok(Node::new(
                        Expression::UnaryOp {
                            op: UnaryOperator::Not,
                            operand: Box::new(operand),
                        },
                        loc,
                    ));
                },
                TokenType::Length => {
                    self.advance()?;
                    let operand = self.parse_unary_expr()?;
                    
                    return Ok(Node::new(
                        Expression::UnaryOp {
                            op: UnaryOperator::Len,
                            operand: Box::new(operand),
                        },
                        loc,
                    ));
                },
                _ => {},
            }
        }
        
        self.parse_power_expr()
    }
    
    /// Parse power expression (^)
    fn parse_power_expr(&mut self) -> Result<Node<Expression>> {
        let mut expr = self.parse_primary_expr()?;
        
        if self.check(&TokenType::Power) {
            let token = self.current_token()?;
            let loc = SourceLoc { line: token.line, column: token.column };
            
            self.advance()?;
            let right = self.parse_unary_expr()?;
            
            expr = Node::new(
                Expression::BinaryOp {
                    op: BinaryOperator::Pow,
                    left: Box::new(expr),
                    right: Box::new(right),
                },
                loc,
            );
        }
        
        Ok(expr)
    }
    
    /// Parse primary expression
    fn parse_primary_expr(&mut self) -> Result<Node<Expression>> {
        let token_clone = self.current_token()?.clone();
        let loc = SourceLoc { line: token_clone.line, column: token_clone.column };
        
        match &token_clone.token_type {
            TokenType::Nil => {
                self.advance()?;
                Ok(Node::new(Expression::Nil, loc))
            },
            TokenType::True => {
                self.advance()?;
                Ok(Node::new(Expression::Boolean(true), loc))
            },
            TokenType::False => {
                self.advance()?;
                Ok(Node::new(Expression::Boolean(false), loc))
            },
            TokenType::Number(n) => {
                let num_val = *n; // Create a copy
                self.advance()?;
                Ok(Node::new(Expression::Number(num_val), loc))
            },
            TokenType::String(s) => {
                let s_clone = s.clone(); // Clone the string
                let string_handle = self.create_string_handle(&s_clone);
                self.advance()?;
                Ok(Node::new(Expression::String(string_handle), loc))
            },
            TokenType::Vararg => {
                self.advance()?;
                Ok(Node::new(Expression::Vararg, loc))
            },
            TokenType::LeftBrace => {
                // Table constructor
                self.parse_table_constructor()
            },
            TokenType::Function => {
                // Anonymous function
                self.advance()?;
                self.parse_anonymous_function(loc)
            },
            TokenType::LeftParen => {
                // Parenthesized expression
                self.advance()?;
                let expr = self.parse_expression()?;
                self.consume(&TokenType::RightParen)?;
                Ok(expr)
            },
            TokenType::Identifier(_) => {
                // Variable reference or function call
                if let Some(expr) = self.try_parse_prefixexp()? {
                    Ok(expr)
                } else {
                    Err(LuaError::SyntaxError {
                        message: "Expected expression".to_string(),
                        line: token_clone.line as usize,
                        column: token_clone.column as usize,
                    })
                }
            },
            _ => {
                Err(LuaError::SyntaxError {
                    message: format!("Unexpected token: {:?}", token_clone.token_type),
                    line: token_clone.line as usize,
                    column: token_clone.column as usize,
                })
            }
        }
    }
    
    /// Parse a table constructor
    fn parse_table_constructor(&mut self) -> Result<Node<Expression>> {
        let token_clone = self.current_token()?.clone();
        let loc = SourceLoc { line: token_clone.line, column: token_clone.column };
        
        self.consume(&TokenType::LeftBrace)?;
        
        let mut fields = Vec::new();
        
        // Empty table
        if self.check(&TokenType::RightBrace) {
            self.advance()?;
            return Ok(Node::new(Expression::TableConstructor(TableConstructor { fields }), loc));
        }
        
        // Parse fields
        loop {
            let field_token = self.current_token()?.clone();
            let field_loc = SourceLoc { line: field_token.line, column: field_token.column };
            
            // Check for key-value pair with expression key: [expr] = expr
            if self.check(&TokenType::LeftBracket) {
                self.advance()?;
                let key = self.parse_expression()?;
                self.consume(&TokenType::RightBracket)?;
                self.consume(&TokenType::Assign)?;
                let value = self.parse_expression()?;
                
                fields.push(TableField::Expression { 
                    key: key,  // Use the whole Node<Expression>, not just key.node
                    value: value 
                });
            }
            // Check for key-value pair with name key: name = expr
            else if let TokenType::Identifier(id) = &field_token.token_type {
                self.advance()?;
                
                if self.check(&TokenType::Assign) {
                    // name = expr
                    self.advance()?;
                    let key = self.create_string_handle(id);
                    let value = self.parse_expression()?;
                    
                    fields.push(TableField::Record { key, value });
                } else {
                    // Just expr (array part)
                    let name = self.create_string_handle(id);
                    let expr = Node::new(Expression::Variable(Variable::Name(name)), field_loc);
                    
                    fields.push(TableField::Array(expr));
                }
            }
            // Regular expression field
            else {
                let expr = self.parse_expression()?;
                fields.push(TableField::Array(expr));
            }
            
            // Check for field separator
            if self.check(&TokenType::Comma) || self.check(&TokenType::Semicolon) {
                self.advance()?;
            } else if self.check(&TokenType::RightBrace) {
                break;
            } else {
                return Err(LuaError::SyntaxError {
                    message: "Expected ',' or ';' or '}' after table field".to_string(),
                    line: self.current_token()?.line as usize,
                    column: self.current_token()?.column as usize,
                });
            }
            
            // End of table
            if self.check(&TokenType::RightBrace) {
                break;
            }
        }
        
        self.consume(&TokenType::RightBrace)?;
        
        Ok(Node::new(Expression::TableConstructor(TableConstructor { fields }), loc))
    }
    
    /// Parse an anonymous function
    fn parse_anonymous_function(&mut self, loc: SourceLoc) -> Result<Node<Expression>> {
        self.consume(&TokenType::LeftParen)?;
        let parameters = self.parse_parameter_list()?;
        self.consume(&TokenType::RightParen)?;
        
        let body = self.parse_chunk()?;
        
        self.consume(&TokenType::End)?;
        
        Ok(Node::new(
            Expression::AnonymousFunction { parameters, body },
            loc,
        ))
    }
    
    /// Try to parse a prefixexp (variable, function call, etc.)
    fn try_parse_prefixexp(&mut self) -> Result<Option<Node<Expression>>> {
        if let Ok(token) = self.current_token() {
            let token_clone = token.clone();
            let loc = SourceLoc { line: token_clone.line, column: token_clone.column };
            
            // Start with a name
            if let TokenType::Identifier(id) = &token_clone.token_type {
                let name = self.create_string_handle(id);
                self.advance()?;
                
                let mut expr = Node::new(Expression::Variable(Variable::Name(name)), loc);
                
                // Parse suffixes (field access, method call, function call)
                loop {
                    if self.check(&TokenType::Dot) {
                        // Table dot access: expr.name
                        self.advance()?;
                        
                        // Get the token before using it in closure
                        let current_token = match self.current_token() {
                            Ok(token) => token.clone(),
                            Err(e) => return Err(e),
                        };
                        
                        if let TokenType::Identifier(id) = &current_token.token_type {
                            let key = self.create_string_handle(id);
                            self.advance()?;
                            
                            match expr.node {
                                Expression::Variable(var) => {
                                    expr = Node::new(
                                        Expression::Variable(Variable::TableDot {
                                            table: Box::new(Node::new(Expression::Variable(var), expr.loc)),
                                            key
                                        }),
                                        loc,
                                    );
                                },
                                _ => {
                                    // For other expressions, wrap in a Variable::TableDot
                                    expr = Node::new(
                                        Expression::Variable(Variable::TableDot {
                                            table: Box::new(expr),
                                            key
                                        }),
                                        loc,
                                    );
                                }
                            }
                        } else {
                            let current_line = current_token.line as usize;
                            let current_column = current_token.column as usize;
                            return Err(LuaError::SyntaxError {
                                message: "Expected identifier after '.'".to_string(),
                                line: current_line,
                                column: current_column,
                            });
                        }
                    } else if self.check(&TokenType::LeftBracket) {
                        // Table bracket access: expr[key]
                        self.advance()?;
                        
                        let key_expr = self.parse_expression()?;
                        
                        self.consume(&TokenType::RightBracket)?;
                        
                        match expr.node {
                            Expression::Variable(var) => {
                                expr = Node::new(
                                    Expression::Variable(Variable::TableField {
                                        table: Box::new(Node::new(Expression::Variable(var), expr.loc)),
                                        key: Box::new(key_expr)
                                    }),
                                    loc,
                                );
                            },
                            _ => {
                                // For other expressions, wrap in a Variable::TableField
                                expr = Node::new(
                                    Expression::Variable(Variable::TableField {
                                        table: Box::new(expr),
                                        key: Box::new(key_expr)
                                    }),
                                    loc,
                                );
                            }
                        }
                    } else if self.check(&TokenType::Colon) {
                        // Method call: expr:name(args)
                        self.advance()?;
                        
                        // Get the token before using it in closure
                        let current_token = match self.current_token() {
                            Ok(token) => token.clone(),
                            Err(e) => return Err(e),
                        };
                        
                        if let TokenType::Identifier(id) = &current_token.token_type {
                            let method_name = self.create_string_handle(id);
                            self.advance()?;
                            
                            // Consume opening parenthesis
                            self.consume(&TokenType::LeftParen)?;
                            
                            // Parse arguments
                            let arguments = if self.check(&TokenType::RightParen) {
                                Vec::new()
                            } else {
                                self.parse_expression_list()?
                            };
                            
                            // Consume closing parenthesis
                            self.consume(&TokenType::RightParen)?;
                            
                            // Create method call expression
                            expr = Node::new(
                                Expression::FunctionCall(FunctionCall {
                                    function: Box::new(expr.clone()),
                                    arguments,
                                    is_method_call: true,
                                    method_name: Some(method_name),
                                }),
                                loc,
                            );
                        } else {
                            let current_line = current_token.line as usize;
                            let current_column = current_token.column as usize;
                            return Err(LuaError::SyntaxError {
                                message: "Expected identifier after ':'".to_string(),
                                line: current_line,
                                column: current_column,
                            });
                        }
                    } else if self.check(&TokenType::LeftParen) {
                        // Function call: expr(args)
                        self.advance()?;
                        
                        // Parse arguments
                        let arguments = if self.check(&TokenType::RightParen) {
                            Vec::new()
                        } else {
                            self.parse_expression_list()?
                        };
                        
                        // Consume closing parenthesis
                        self.consume(&TokenType::RightParen)?;
                        
                        // Create function call expression
                        expr = Node::new(
                            Expression::FunctionCall(FunctionCall {
                                function: Box::new(expr.clone()),
                                arguments,
                                is_method_call: false,
                                method_name: None,
                            }),
                            loc,
                        );
                    } else {
                        // No more suffixes
                        break;
                    }
                }
                
                return Ok(Some(expr));
            }
        }
        
        Ok(None)
    }
}