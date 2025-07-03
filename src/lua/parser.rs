//! Lua Parser Module
//!
//! This module implements the parser for Lua code, converting tokens
//! from the lexer into an Abstract Syntax Tree (AST).

use super::error::{LuaError, LuaResult};
use super::lexer::{Token, TokenWithLocation, tokenize};
use super::ast::*;

/// Parser for Lua source code
pub struct Parser {
    /// Tokens to parse
    tokens: Vec<TokenWithLocation>,
    /// Current token position
    current: usize,
}

impl Parser {
    /// Create a new parser for the given tokens
    pub fn new(tokens: Vec<TokenWithLocation>) -> Self {
        Parser {
            tokens,
            current: 0,
        }
    }
    
    /// Parse the tokens into a chunk
    pub fn parse(&mut self) -> LuaResult<Chunk> {
        let mut chunk = Chunk::new();
        
        // Parse statements until we hit the end or a return
        while !self.check(&Token::Eof) && !self.check(&Token::Return) {
            chunk.statements.push(self.statement()?);
        }
        
        // Parse return statement if present
        if self.match_token(Token::Return) {
            let mut expressions = Vec::new();
            
            // Parse return values
            if !self.check_statement_end() {
                expressions.push(self.expression()?);
                
                while self.match_token(Token::Comma) {
                    expressions.push(self.expression()?);
                }
            }
            
            chunk.return_statement = Some(ReturnStatement { expressions });
            
            // Optional semicolon after return
            self.match_token(Token::Semicolon);
        }
        
        // Consume the EOF token
        self.consume(Token::Eof, "Expected end of file")?;
        
        Ok(chunk)
    }
    
    /// Parse a statement
    fn statement(&mut self) -> LuaResult<Statement> {
        // Match the statement type
        if self.match_token(Token::Semicolon) {
            // Empty statement
            self.statement()
        } else if self.match_token(Token::If) {
            self.if_statement()
        } else if self.match_token(Token::While) {
            self.while_statement()
        } else if self.match_token(Token::Do) {
            self.do_statement()
        } else if self.match_token(Token::For) {
            self.for_statement()
        } else if self.match_token(Token::Repeat) {
            self.repeat_statement()
        } else if self.match_token(Token::Function) {
            self.function_statement()
        } else if self.match_token(Token::Local) {
            self.local_statement()
        } else if self.match_token(Token::DoubleColon) {
            self.label_statement()
        } else if self.match_token(Token::Return) {
            // Parse a Return statement - can now appear inside functions
            let mut expressions = Vec::new();
            
            // Parse return values if not at end of statement
            if !self.check_statement_end() {
                expressions.push(self.expression()?);
                
                while self.match_token(Token::Comma) {
                    expressions.push(self.expression()?);
                }
            }
            
            // Optional semicolon after return
            self.match_token(Token::Semicolon);
            
            Ok(Statement::Return { expressions })
        } else if self.match_token(Token::Break) {
            self.break_statement()
        } else if self.match_token(Token::Goto) {
            self.goto_statement()
        } else {
            // Must be an assignment or function call
            let expr = self.primary_expression()?;
            
            if let Expression::FunctionCall(call) = expr {
                // Function call statement
                Ok(Statement::FunctionCall(*call))
            } else {
                // Assignment statement
                self.finish_assignment(expr)
            }
        }
    }
    
    /// Parse a block of statements
    fn block(&mut self) -> LuaResult<Block> {
        let mut block = Block::new();
        
        // Parse statements until end token or other block terminator
        while !self.check_block_end() {
            block.statements.push(self.statement()?);
        }
        
        Ok(block)
    }
    
    /// Parse an if statement
    fn if_statement(&mut self) -> LuaResult<Statement> {
        // Parse condition
        let condition = self.expression()?;
        
        // Parse "then"
        self.consume(Token::Then, "Expected 'then' after if condition")?;
        
        // Parse main block
        let body = self.block()?;
        
        // Parse optional elseif/else parts
        let mut else_ifs = Vec::new();
        let mut else_block = None;
        
        while self.match_token(Token::ElseIf) {
            let else_if_condition = self.expression()?;
            self.consume(Token::Then, "Expected 'then' after elseif condition")?;
            let else_if_body = self.block()?;
            else_ifs.push((else_if_condition, else_if_body));
        }
        
        if self.match_token(Token::Else) {
            else_block = Some(self.block()?);
        }
        
        // Parse end
        self.consume(Token::End, "Expected 'end' after if block")?;
        
        Ok(Statement::If {
            condition,
            body,
            else_ifs,
            else_block,
        })
    }
    
    /// Parse a while statement
    fn while_statement(&mut self) -> LuaResult<Statement> {
        // Parse condition
        let condition = self.expression()?;
        
        // Parse "do"
        self.consume(Token::Do, "Expected 'do' after while condition")?;
        
        // Parse body
        let body = self.block()?;
        
        // Parse end
        self.consume(Token::End, "Expected 'end' after while block")?;
        
        Ok(Statement::While {
            condition,
            body,
        })
    }
    
    /// Parse a repeat statement
    fn repeat_statement(&mut self) -> LuaResult<Statement> {
        // Parse body
        let body = self.block()?;
        
        // Parse until
        self.consume(Token::Until, "Expected 'until' after repeat block")?;
        
        // Parse condition
        let condition = self.expression()?;
        
        Ok(Statement::Repeat {
            body,
            condition,
        })
    }
    
    /// Parse a do statement
    fn do_statement(&mut self) -> LuaResult<Statement> {
        // Parse body
        let body = self.block()?;
        
        // Parse end
        self.consume(Token::End, "Expected 'end' after do block")?;
        
        Ok(Statement::Do(body))
    }
    
    /// Parse a for statement (numeric or generic)
    fn for_statement(&mut self) -> LuaResult<Statement> {
        // Parse variable name
        let var_token = self.consume_identifier("Expected variable name in for loop")?;
        
        // Check if it's a numeric or generic for
        if self.match_token(Token::Assign) {
            // Numeric for
            let initial = self.expression()?;
            
            self.consume(Token::Comma, "Expected ',' after for initial value")?;
            let limit = self.expression()?;
            
            let step = if self.match_token(Token::Comma) {
                Some(self.expression()?)
            } else {
                None
            };
            
            self.consume(Token::Do, "Expected 'do' after for limits")?;
            
            let body = self.block()?;
            
            self.consume(Token::End, "Expected 'end' after for block")?;
            
            Ok(Statement::ForLoop {
                variable: var_token,
                initial,
                limit,
                step,
                body,
            })
        } else {
            // Generic for
            let mut variables = vec![var_token];
            
            // Parse additional variables
            while self.match_token(Token::Comma) {
                variables.push(self.consume_identifier("Expected variable name after ','")?);
            }
            
            self.consume(Token::In, "Expected 'in' after for variables")?;
            
            // Parse iterator expressions
            let mut iterators = vec![self.expression()?];
            
            while self.match_token(Token::Comma) {
                iterators.push(self.expression()?);
            }
            
            self.consume(Token::Do, "Expected 'do' after for iterators")?;
            
            let body = self.block()?;
            
            self.consume(Token::End, "Expected 'end' after for block")?;
            
            Ok(Statement::ForInLoop {
                variables,
                iterators,
                body,
            })
        }
    }
    
    /// Parse a function statement
    fn function_statement(&mut self) -> LuaResult<Statement> {
        // Parse function name
        let name = self.function_name()?;
        
        // Parse parameters and body
        let (parameters, is_vararg, body) = self.function_body()?;
        
        Ok(Statement::FunctionDefinition {
            name,
            parameters,
            is_vararg,
            body,
        })
    }
    
    /// Parse a function name
    fn function_name(&mut self) -> LuaResult<FunctionName> {
        // Parse first part of name
        let first = self.consume_identifier("Expected function name")?;
        let mut name_parts = vec![first];
        
        // Parse additional parts (a.b.c)
        while self.match_token(Token::Dot) {
            name_parts.push(self.consume_identifier("Expected identifier after '.'")?);
        }
        
        // Parse optional method part (a.b.c:d)
        let method = if self.match_token(Token::Colon) {
            Some(self.consume_identifier("Expected method name after ':'")?) 
        } else {
            None
        };
        
        Ok(FunctionName {
            names: name_parts,
            method,
        })
    }
    
    /// Parse a function body
    fn function_body(&mut self) -> LuaResult<(Vec<String>, bool, Block)> {
        // Parse opening parenthesis
        self.consume(Token::LeftParen, "Expected '(' after function name")?;
        
        // Parse parameters
        let mut parameters = Vec::new();
        let mut is_vararg = false;
        
        // Handle empty parameter list
        if !self.check(&Token::RightParen) {
            // Parse first parameter
            if self.match_token(Token::TripleDot) {
                is_vararg = true;
            } else {
                parameters.push(self.consume_identifier("Expected parameter name")?);
                
                // Parse additional parameters
                while self.match_token(Token::Comma) {
                    if self.match_token(Token::TripleDot) {
                        is_vararg = true;
                        break;
                    } else {
                        parameters.push(self.consume_identifier("Expected parameter name")?);
                    }
                }
            }
        }
        
        // Parse closing parenthesis
        self.consume(Token::RightParen, "Expected ')' after function parameters")?;
        
        // Parse function body
        let body = self.block()?;
        
        // Parse end
        self.consume(Token::End, "Expected 'end' after function body")?;
        
        Ok((parameters, is_vararg, body))
    }
    
    /// Parse a local statement (local declaration or local function)
    fn local_statement(&mut self) -> LuaResult<Statement> {
        if self.match_token(Token::Function) {
            // Local function
            let name = self.consume_identifier("Expected function name")?;
            
            // Parse parameters and body
            let (parameters, is_vararg, body) = self.function_body()?;
            
            Ok(Statement::LocalFunctionDefinition {
                name,
                parameters,
                is_vararg,
                body,
            })
        } else {
            // Local declaration
            let mut names = vec![self.consume_identifier("Expected variable name")?];
            
            // Parse additional names
            while self.match_token(Token::Comma) {
                names.push(self.consume_identifier("Expected variable name")?);
            }
            
            // Parse optional initializer
            let expressions = if self.match_token(Token::Assign) {
                let mut exprs = vec![self.expression()?];
                
                while self.match_token(Token::Comma) {
                    exprs.push(self.expression()?);
                }
                
                exprs
            } else {
                Vec::new()
            };
            
            Ok(Statement::LocalDeclaration(LocalDeclaration {
                names,
                expressions,
            }))
        }
    }
    
    /// Parse a label statement
    fn label_statement(&mut self) -> LuaResult<Statement> {
        // Parse label name
        let name = self.consume_identifier("Expected label name")?;
        
        // Parse closing ::
        self.consume(Token::DoubleColon, "Expected '::' after label name")?;
        
        Ok(Statement::LabelDefinition(name))
    }
    
    /// Parse a break statement
    fn break_statement(&mut self) -> LuaResult<Statement> {
        // Optional semicolon after break
        self.match_token(Token::Semicolon);
        
        Ok(Statement::Break)
    }
    
    /// Parse a goto statement
    fn goto_statement(&mut self) -> LuaResult<Statement> {
        // Parse label name
        let name = self.consume_identifier("Expected label name")?;
        
        // Optional semicolon
        self.match_token(Token::Semicolon);
        
        Ok(Statement::Goto(name))
    }
    
    /// Parse an assignment statement
    fn finish_assignment(&mut self, first_var_expr: Expression) -> LuaResult<Statement> {
        // First, convert the expression to a variable
        let first_var = match first_var_expr {
            Expression::Variable(var) => var,
            _ => {
                return Err(LuaError::SyntaxError {
                    message: "Expected variable before '='".to_string(),
                    line: self.current_line(),
                    column: self.current_column(),
                });
            }
        };
        
        // Collect variables
        let mut variables = vec![first_var];
        
        while self.match_token(Token::Comma) {
            let expr = self.primary_expression()?;
            if let Expression::Variable(var) = expr {
                variables.push(var);
            } else {
                return Err(LuaError::SyntaxError {
                    message: "Expected variable before '='".to_string(),
                    line: self.current_line(),
                    column: self.current_column(),
                });
            }
        }
        
        // Parse assignment operator
        self.consume(Token::Assign, "Expected '=' after variable")?;
        
        // Parse expressions
        let mut expressions = vec![self.expression()?];
        
        while self.match_token(Token::Comma) {
            expressions.push(self.expression()?);
        }
        
        Ok(Statement::Assignment(Assignment {
            variables,
            expressions,
        }))
    }
    
    /// Parse an expression
    fn expression(&mut self) -> LuaResult<Expression> {
        self.or_expr()
    }
    
    /// Parse an 'or' expression
    fn or_expr(&mut self) -> LuaResult<Expression> {
        let mut expr = self.and_expr()?;
        
        while self.match_token(Token::Or) {
            let right = self.and_expr()?;
            expr = Expression::BinaryOp {
                left: Box::new(expr),
                operator: BinaryOperator::Or,
                right: Box::new(right),
            };
        }
        
        Ok(expr)
    }
    
    /// Parse an 'and' expression
    fn and_expr(&mut self) -> LuaResult<Expression> {
        let mut expr = self.comparison()?;
        
        while self.match_token(Token::And) {
            let right = self.comparison()?;
            expr = Expression::BinaryOp {
                left: Box::new(expr),
                operator: BinaryOperator::And,
                right: Box::new(right),
            };
        }
        
        Ok(expr)
    }
    
    /// Parse a comparison expression
    fn comparison(&mut self) -> LuaResult<Expression> {
        let mut expr = self.concat()?;
        
        loop {
            let op = if self.match_token(Token::Equal) {
                BinaryOperator::Eq
            } else if self.match_token(Token::NotEqual) {
                BinaryOperator::Ne
            } else if self.match_token(Token::LessThan) {
                BinaryOperator::Lt
            } else if self.match_token(Token::LessEqual) {
                BinaryOperator::Le
            } else if self.match_token(Token::GreaterThan) {
                BinaryOperator::Gt
            } else if self.match_token(Token::GreaterEqual) {
                BinaryOperator::Ge
            } else {
                break;
            };
            
            let right = self.concat()?;
            expr = Expression::BinaryOp {
                left: Box::new(expr),
                operator: op,
                right: Box::new(right),
            };
            
            // Only allow one comparison operator
            break;
        }
        
        Ok(expr)
    }
    
    /// Parse string concatenation
    fn concat(&mut self) -> LuaResult<Expression> {
        let mut expr = self.addition()?;
        
        while self.match_token(Token::DoubleDot) {
            let right = self.addition()?;
            expr = Expression::BinaryOp {
                left: Box::new(expr),
                operator: BinaryOperator::Concat,
                right: Box::new(right),
            };
        }
        
        Ok(expr)
    }
    
    /// Parse addition and subtraction
    fn addition(&mut self) -> LuaResult<Expression> {
        let mut expr = self.multiplication()?;
        
        loop {
            let op = if self.match_token(Token::Plus) {
                BinaryOperator::Add
            } else if self.match_token(Token::Minus) {
                BinaryOperator::Sub
            } else {
                break;
            };
            
            let right = self.multiplication()?;
            expr = Expression::BinaryOp {
                left: Box::new(expr),
                operator: op,
                right: Box::new(right),
            };
        }
        
        Ok(expr)
    }
    
    /// Parse multiplication, division, and modulo
    fn multiplication(&mut self) -> LuaResult<Expression> {
        let mut expr = self.unary()?;
        
        loop {
            let op = if self.match_token(Token::Mul) {
                BinaryOperator::Mul
            } else if self.match_token(Token::Div) {
                BinaryOperator::Div
            } else if self.match_token(Token::Mod) {
                BinaryOperator::Mod
            } else {
                break;
            };
            
            let right = self.unary()?;
            expr = Expression::BinaryOp {
                left: Box::new(expr),
                operator: op,
                right: Box::new(right),
            };
        }
        
        Ok(expr)
    }
    
    /// Parse unary operators
    fn unary(&mut self) -> LuaResult<Expression> {
        if self.match_token(Token::Not) {
            let operand = self.unary()?;
            Ok(Expression::UnaryOp {
                operator: UnaryOperator::Not,
                operand: Box::new(operand),
            })
        } else if self.match_token(Token::Minus) {
            let operand = self.unary()?;
            Ok(Expression::UnaryOp {
                operator: UnaryOperator::Minus,
                operand: Box::new(operand),
            })
        } else if self.match_token(Token::Hash) {
            let operand = self.unary()?;
            Ok(Expression::UnaryOp {
                operator: UnaryOperator::Length,
                operand: Box::new(operand),
            })
        } else {
            self.power()
        }
    }
    
    /// Parse exponentiation
    fn power(&mut self) -> LuaResult<Expression> {
        let mut expr = self.primary_expression()?;
        
        while self.match_token(Token::Pow) {
            let right = self.unary()?; // Right-associative
            expr = Expression::BinaryOp {
                left: Box::new(expr),
                operator: BinaryOperator::Pow,
                right: Box::new(right),
            };
        }
        
        Ok(expr)
    }
    
    /// Parse primary expressions (literals, variables, etc.)
    fn primary_expression(&mut self) -> LuaResult<Expression> {
        let token = self.peek_token().clone();
        
        match &token.token {
            Token::Nil => {
                self.advance();
                Ok(Expression::Nil)
            }
            Token::True => {
                self.advance();
                Ok(Expression::Boolean(true))
            }
            Token::False => {
                self.advance();
                Ok(Expression::Boolean(false))
            }
            Token::Number(n) => {
                self.advance();
                Ok(Expression::Number(*n))
            }
            Token::String(s) => {
                self.advance();
                Ok(Expression::String(s.clone()))
            }
            Token::TripleDot => {
                self.advance();
                Ok(Expression::VarArg)
            }
            Token::LeftBrace => {
                self.advance();
                self.table_constructor()
            }
            Token::Function => {
                self.advance();
                self.function_expression()
            }
            Token::LeftParen => {
                self.advance();
                let expr = self.expression()?;
                self.consume(Token::RightParen, "Expected ')' after expression")?;
                Ok(expr)
            }
            _ => {
                // Must be a variable or function call
                self.variable_or_call()
            }
        }
    }
    
    /// Parse a variable or function call
    fn variable_or_call(&mut self) -> LuaResult<Expression> {
        let mut expr = self.primary_variable()?;
        
        loop {
            if self.match_token(Token::Dot) {
                // Table field access (a.b)
                let field = self.consume_identifier("Expected field name after '.'")?;
                expr = Expression::Variable(Variable::Member {
                    table: Box::new(expr),
                    field,
                });
            } else if self.match_token(Token::LeftBracket) {
                // Table index (a[b])
                let index = self.expression()?;
                self.consume(Token::RightBracket, "Expected ']' after index")?;
                expr = Expression::Variable(Variable::Index {
                    table: Box::new(expr),
                    key: Box::new(index),
                });
            } else if self.match_token(Token::Colon) {
                // Method call (a:b())
                let method = self.consume_identifier("Expected method name after ':'")?;
                
                // Parse arguments
                let args = self.call_args()?;
                
                expr = Expression::FunctionCall(Box::new(FunctionCall {
                    function: expr,
                    method: Some(method),
                    args,
                }));
            } else if self.check(&Token::LeftParen) || 
                      self.check_string_token() || 
                      self.check(&Token::LeftBrace) {
                // Function call (a())
                let args = self.call_args()?;
                
                expr = Expression::FunctionCall(Box::new(FunctionCall {
                    function: expr,
                    method: None,
                    args,
                }));
            } else {
                // No more indexing or calls
                break;
            }
        }
        
        Ok(expr)
    }
    
    /// Parse a primary variable (just a name)
    fn primary_variable(&mut self) -> LuaResult<Expression> {
        let token = self.peek_token().clone();
        
        if let Token::Identifier(name) = &token.token {
            let name = name.clone();
            self.advance();
            Ok(Expression::Variable(Variable::Name(name)))
        } else {
            Err(LuaError::SyntaxError {
                message: "Expected variable name".to_string(),
                line: token.line,
                column: token.column,
            })
        }
    }
    
    /// Check if the current token matches one of the string token types
    fn check_string_token(&self) -> bool {
        if self.is_at_end() {
            return false;
        }
        
        matches!(self.peek_token().token, Token::String(_))
    }
    
    /// Parse function call arguments
    fn call_args(&mut self) -> LuaResult<CallArgs> {
        if self.match_token(Token::LeftParen) {
            // Regular argument list
            let mut args = Vec::new();
            
            if !self.check(&Token::RightParen) {
                args.push(self.expression()?);
                
                while self.match_token(Token::Comma) {
                    args.push(self.expression()?);
                }
            }
            
            self.consume(Token::RightParen, "Expected ')' after function arguments")?;
            
            Ok(CallArgs::Args(args))
        } else if let Token::String(s) = &self.peek_token().token {
            // String literal as single argument
            let string = s.clone();
            self.advance(); // Skip over the string token
            Ok(CallArgs::String(string))
        } else if self.match_token(Token::LeftBrace) {
            // Table constructor as single argument
            let table = self.finish_table_constructor()?;
            Ok(CallArgs::Table(table))
        } else {
            Err(LuaError::SyntaxError {
                message: "Expected '(', string, or table after function".to_string(),
                line: self.current_line(),
                column: self.current_column(),
            })
        }
    }
    
    /// Parse a function expression
    fn function_expression(&mut self) -> LuaResult<Expression> {
        // Parse opening parenthesis
        self.consume(Token::LeftParen, "Expected '(' after function")?;
        
        // Parse parameters
        let mut parameters = Vec::new();
        let mut is_vararg = false;
        
        if !self.check(&Token::RightParen) {
            // Parse first parameter
            if self.match_token(Token::TripleDot) {
                is_vararg = true;
            } else {
                parameters.push(self.consume_identifier("Expected parameter name")?);
                
                // Parse additional parameters
                while self.match_token(Token::Comma) {
                    if self.match_token(Token::TripleDot) {
                        is_vararg = true;
                        break;
                    } else {
                        parameters.push(self.consume_identifier("Expected parameter name")?);
                    }
                }
            }
        }
        
        // Parse closing parenthesis
        self.consume(Token::RightParen, "Expected ')' after function parameters")?;
        
        // Parse function body
        let body = self.block()?;
        
        // Parse end
        self.consume(Token::End, "Expected 'end' after function body")?;
        
        Ok(Expression::FunctionDef {
            parameters,
            is_vararg,
            body,
        })
    }
    
    /// Parse a table constructor
    fn table_constructor(&mut self) -> LuaResult<Expression> {
        let table = self.finish_table_constructor()?;
        Ok(Expression::TableConstructor(table))
    }
    
    /// Parse the contents of a table constructor
    fn finish_table_constructor(&mut self) -> LuaResult<TableConstructor> {
        let mut table = TableConstructor::new();
        
        // Parse fields until we hit the end
        if !self.check(&Token::RightBrace) {
            // Parse first field
            table.fields.push(self.table_field()?);
            
            // Parse field separator and additional fields
            while self.match_token(Token::Comma) || self.match_token(Token::Semicolon) {
                if self.check(&Token::RightBrace) {
                    break;
                }
                
                table.fields.push(self.table_field()?);
            }
        }
        
        // Parse closing brace
        self.consume(Token::RightBrace, "Expected '}' after table constructor")?;
        
        Ok(table)
    }
    
    /// Parse a table field
    fn table_field(&mut self) -> LuaResult<TableField> {
        if self.check(&Token::LeftBracket) {
            // Indexed field: [expr] = value
            self.advance();
            let key = self.expression()?;
            self.consume(Token::RightBracket, "Expected ']' after table key")?;
            self.consume(Token::Assign, "Expected '=' after table key")?;
            let value = self.expression()?;
            
            Ok(TableField::Index { key, value })
        } else {
            // Try to parse as a named field: name = value
            let pos = self.current;
            
            if let Ok(name) = self.consume_identifier("") {
                if self.match_token(Token::Assign) {
                    // Named field
                    let value = self.expression()?;
                    return Ok(TableField::Record { key: name, value });
                } else {
                    // Backtrack, treat as list element
                    self.current = pos;
                }
            }
            
            // List field: value
            let expr = self.expression()?;
            Ok(TableField::List(expr))
        }
    }
    
    /// Check if the current token matches the given token
    fn check(&self, token: &Token) -> bool {
        if self.is_at_end() {
            return *token == Token::Eof;
        }
        
        self.peek_token().token == *token
    }
    
    /// Match the current token and advance if it matches
    fn match_token(&mut self, token: Token) -> bool {
        if self.check(&token) {
            self.advance();
            true
        } else {
            false
        }
    }
    
    /// Consume a token if it matches, otherwise error
    fn consume(&mut self, token: Token, message: &str) -> LuaResult<()> {
        if self.check(&token) {
            self.advance();
            Ok(())
        } else {
            let token_loc = self.peek_token().clone();
            Err(LuaError::SyntaxError {
                message: format!("{}: expected {:?}, got {:?}", 
                                message, token, token_loc.token),
                line: token_loc.line,
                column: token_loc.column,
            })
        }
    }
    
    /// Consume an identifier token
    fn consume_identifier(&mut self, message: &str) -> LuaResult<String> {
        let token_loc = self.peek_token().clone();
        
        if let Token::Identifier(name) = &token_loc.token {
            let name = name.clone();
            self.advance();
            Ok(name)
        } else {
            Err(LuaError::SyntaxError {
                message: if message.is_empty() {
                    "Expected identifier".to_string()
                } else {
                    message.to_string()
                },
                line: token_loc.line,
                column: token_loc.column,
            })
        }
    }
    
    /// Advance to the next token
    fn advance(&mut self) -> &TokenWithLocation {
        if !self.is_at_end() {
            self.current += 1;
        }
        self.previous()
    }
    
    /// Get the current token
    fn peek_token(&self) -> &TokenWithLocation {
        &self.tokens[self.current]
    }
    
    /// Get the previous token
    fn previous(&self) -> &TokenWithLocation {
        &self.tokens[self.current - 1]
    }
    
    /// Check if we've reached the end of the tokens
    fn is_at_end(&self) -> bool {
        self.current >= self.tokens.len() || 
            self.tokens[self.current].token == Token::Eof
    }
    
    /// Check if the current token is a statement terminator
    fn check_statement_end(&self) -> bool {
        self.check(&Token::Semicolon) || 
            self.check(&Token::End) ||
            self.check(&Token::Else) ||
            self.check(&Token::ElseIf) ||
            self.check(&Token::Until) ||
            self.check(&Token::Eof)
    }
    
    /// Check if the current token is a block terminator
    fn check_block_end(&self) -> bool {
        self.check(&Token::End) ||
            self.check(&Token::Else) ||
            self.check(&Token::ElseIf) ||
            self.check(&Token::Until) ||
            self.check(&Token::Eof)
    }
    
    /// Get the current line
    fn current_line(&self) -> usize {
        self.peek_token().line
    }
    
    /// Get the current column
    fn current_column(&self) -> usize {
        self.peek_token().column
    }
}

/// Parse Lua source code into an AST
pub fn parse(source: &str) -> LuaResult<Chunk> {
    // Tokenize the source
    let tokens = tokenize(source)?;
    
    // Parse the tokens
    let mut parser = Parser::new(tokens);
    parser.parse()
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_parse_simple() {
        let source = "local x = 42";
        let chunk = parse(source).unwrap();
        
        assert_eq!(chunk.statements.len(), 1);
        assert_eq!(chunk.return_statement, None);
        
        match &chunk.statements[0] {
            Statement::LocalDeclaration(decl) => {
                assert_eq!(decl.names.len(), 1);
                assert_eq!(decl.names[0], "x");
                assert_eq!(decl.expressions.len(), 1);
                assert!(matches!(&decl.expressions[0], Expression::Number(42.0)));
            },
            _ => panic!("Expected LocalDeclaration"),
        }
    }
    
    #[test]
    fn test_parse_function() {
        let source = "function add(a, b) return a + b end";
        let chunk = parse(source).unwrap();
        
        assert_eq!(chunk.statements.len(), 1);
        
        match &chunk.statements[0] {
            Statement::FunctionDefinition { name, parameters, is_vararg, body } => {
                assert_eq!(name.names.len(), 1);
                assert_eq!(name.names[0], "add");
                assert_eq!(name.method, None);
                
                assert_eq!(parameters.len(), 2);
                assert_eq!(parameters[0], "a");
                assert_eq!(parameters[1], "b");
                
                assert_eq!(*is_vararg, false);
            },
            _ => panic!("Expected FunctionDefinition"),
        }
    }
    
    #[test]
    fn test_parse_if() {
        let source = "if x > 0 then return 1 elseif x < 0 then return -1 else return 0 end";
        let chunk = parse(source).unwrap();
        
        assert_eq!(chunk.statements.len(), 1);
        
        match &chunk.statements[0] {
            Statement::If { condition, body, else_ifs, else_block } => {
                assert!(matches!(condition, Expression::BinaryOp { .. }));
                assert_eq!(else_ifs.len(), 1);
                assert!(else_block.is_some());
            },
            _ => panic!("Expected If statement"),
        }
    }
    
    #[test]
    fn test_parse_table() {
        let source = "local t = { foo = 'bar', [1] = 42, 'list_item' }";
        let chunk = parse(source).unwrap();
        
        match &chunk.statements[0] {
            Statement::LocalDeclaration(decl) => {
                match &decl.expressions[0] {
                    Expression::TableConstructor(table) => {
                        assert_eq!(table.fields.len(), 3);
                    },
                    _ => panic!("Expected TableConstructor"),
                }
            },
            _ => panic!("Expected LocalDeclaration"),
        }
    }
}