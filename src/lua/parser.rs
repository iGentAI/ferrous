//! Lua parser
//!
//! This module converts a stream of tokens into an abstract syntax tree (AST).

use super::ast::*;
use super::error::{LuaError, Result};
use super::lexer::{Lexer, Token};
use super::value::LuaString;
use std::iter::Peekable;

/// Parser for Lua source code
pub struct Parser<'a> {
    /// The lexer providing tokens
    lexer: Peekable<TokenIterator<'a>>,
    
    /// The current token
    current: Token,
    
    /// Current position for error reporting
    position: Location,
}

/// Iterator wrapper for lexer
struct TokenIterator<'a> {
    lexer: Lexer<'a>,
}

impl<'a> Iterator for TokenIterator<'a> {
    type Item = Result<(Token, Location)>;
    
    fn next(&mut self) -> Option<Self::Item> {
        let line = self.lexer.line();
        let column = self.lexer.column();
        let token = self.lexer.next_token();
        
        match token {
            Ok(Token::Eof) => None,
            Ok(token) => Some(Ok((token, Location::new(line, column)))),
            Err(err) => Some(Err(err)),
        }
    }
}



impl<'a> Parser<'a> {
    /// Create a new parser for the given input
    pub fn new(input: &'a str) -> Result<Self> {
        let mut lexer = Lexer::new(input);
        let first_token = lexer.next_token()?;
        
        Ok(Parser {
            lexer: TokenIterator { lexer }.peekable(),
            current: first_token,
            position: Location::new(1, 1),
        })
    }
    
    /// Parse the input into a chunk
    pub fn parse(&mut self) -> Result<Chunk> {
        let block = self.parse_block()?;
        Ok(Chunk { block })
    }
    
    /// Parse a block of statements
    fn parse_block(&mut self) -> Result<Block> {
        let mut statements = Vec::new();
        let mut return_stmt = None;
        
        // Parse statements until a block terminator is encountered
        while !self.is_block_terminator() {
            if self.check(&Token::Return) {
                return_stmt = Some(self.parse_return_statement()?);
                
                // After a return, only a semicolon is allowed before the end
                if self.check(&Token::Semicolon) {
                    self.advance()?;
                }
                break;
            }
            
            let stmt = self.parse_statement()?;
            statements.push(stmt);
            
            // Optional semicolon
            if self.check(&Token::Semicolon) {
                self.advance()?;
            }
        }
        
        Ok(Block { statements, return_stmt })
    }
    
    /// Check if the current token is a block terminator
    fn is_block_terminator(&self) -> bool {
        match self.current {
            Token::End | Token::Else | Token::Elseif | Token::Until | Token::Eof => true,
            _ => false,
        }
    }
    
    /// Parse a single statement
    fn parse_statement(&mut self) -> Result<Statement> {
        match &self.current {
            Token::Semicolon => {
                self.advance()?;
                Ok(Statement::Empty)
            },
            
            Token::If => self.parse_if_statement(),
            Token::While => self.parse_while_statement(),
            Token::Repeat => self.parse_repeat_statement(),
            Token::For => self.parse_for_statement(),
            Token::Function => self.parse_function_statement(),
            Token::Local => self.parse_local_statement(),
            Token::Do => self.parse_do_statement(),
            Token::Break => {
                self.advance()?;
                Ok(Statement::Break)
            },
            
            // Variable assignment or function call
            Token::Identifier(_) => {
                // Need to look ahead to see if this is an assignment or function call
                let var = self.parse_prefix_exp()?;
                
                match var {
                    Expression::FunctionCall(call) => {
                        Ok(Statement::FunctionCall(call))
                    },
                    Expression::Variable(var) => {
                        // This is an assignment
                        let mut vars = vec![var];
                        
                        // Parse more vars if there's a comma
                        while self.check(&Token::Comma) {
                            self.advance()?;
                            match self.parse_prefix_exp()? {
                                Expression::Variable(v) => vars.push(v),
                                _ => return Err(LuaError::Syntax("expected variable in assignment".to_string())),
                            }
                        }
                        
                        // Expect = after variables
                        self.expect(Token::Assign, "expected '=' in assignment")?;
                        
                        // Parse expressions
                        let values = self.parse_expression_list()?;
                        
                        Ok(Statement::Assignment(AssignmentStatement { vars, values }))
                    },
                    _ => Err(LuaError::Syntax("expected variable or function call".to_string())),
                }
            },
            
            _ => Err(LuaError::Syntax(format!("unexpected token in statement: {:?}", self.current))),
        }
    }
    
    /// Parse an if statement
    fn parse_if_statement(&mut self) -> Result<Statement> {
        // Parse if condition
        self.advance()?; // Skip 'if'
        let condition = self.parse_expression()?;
        self.expect(Token::Then, "expected 'then' after if condition")?;
        
        // Parse then block
        let then_block = self.parse_block()?;
        
        // Parse elseif branches
        let mut elseif_branches = Vec::new();
        while self.check(&Token::Elseif) {
            self.advance()?; // Skip 'elseif'
            let elseif_cond = self.parse_expression()?;
            self.expect(Token::Then, "expected 'then' after elseif condition")?;
            let elseif_block = self.parse_block()?;
            elseif_branches.push((elseif_cond, elseif_block));
        }
        
        // Parse optional else branch
        let else_block = if self.check(&Token::Else) {
            self.advance()?; // Skip 'else'
            Some(self.parse_block()?)
        } else {
            None
        };
        
        // Expect end
        self.expect(Token::End, "expected 'end' to close if statement")?;
        
        Ok(Statement::If(IfStatement {
            condition,
            then_block,
            elseif_branches,
            else_block,
        }))
    }
    
    /// Parse a while statement
    fn parse_while_statement(&mut self) -> Result<Statement> {
        self.advance()?; // Skip 'while'
        let condition = self.parse_expression()?;
        self.expect(Token::Do, "expected 'do' after while condition")?;
        
        let body = self.parse_block()?;
        
        self.expect(Token::End, "expected 'end' to close while loop")?;
        
        Ok(Statement::While {
            condition,
            body,
        })
    }
    
    /// Parse a repeat statement
    fn parse_repeat_statement(&mut self) -> Result<Statement> {
        self.advance()?; // Skip 'repeat'
        
        let body = self.parse_block()?;
        
        self.expect(Token::Until, "expected 'until' after repeat body")?;
        
        let condition = self.parse_expression()?;
        
        Ok(Statement::Repeat {
            body,
            condition,
        })
    }
    
    /// Parse a for statement (numeric or generic)
    fn parse_for_statement(&mut self) -> Result<Statement> {
        self.advance()?; // Skip 'for'
        
        // Get the first name
        let name = self.parse_name()?;
        
        // Check if this is a numeric or generic for
        if self.check(&Token::Assign) {
            // Numeric for
            self.advance()?; // Skip '='
            
            let start = self.parse_expression()?;
            
            self.expect(Token::Comma, "expected ',' after for start value")?;
            
            let end = self.parse_expression()?;
            
            // Optional step
            let step = if self.check(&Token::Comma) {
                self.advance()?; // Skip ','
                Some(self.parse_expression()?)
            } else {
                None
            };
            
            self.expect(Token::Do, "expected 'do' after for range")?;
            
            let body = self.parse_block()?;
            
            self.expect(Token::End, "expected 'end' to close for loop")?;
            
            Ok(Statement::NumericFor {
                var: name,
                start,
                end,
                step,
                body,
            })
        } else {
            // Generic for
            let mut names = vec![name];
            
            // Parse additional names
            while self.check(&Token::Comma) {
                self.advance()?; // Skip ','
                names.push(self.parse_name()?);
            }
            
            self.expect(Token::In, "expected 'in' in generic for")?;
            
            let iterators = self.parse_expression_list()?;
            
            self.expect(Token::Do, "expected 'do' after for iterators")?;
            
            let body = self.parse_block()?;
            
            self.expect(Token::End, "expected 'end' to close for loop")?;
            
            Ok(Statement::GenericFor {
                vars: names,
                iterators,
                body,
            })
        }
    }
    
    /// Parse a local statement (local variable or function)
    fn parse_local_statement(&mut self) -> Result<Statement> {
        self.advance()?; // Skip 'local'
        
        if self.check(&Token::Function) {
            self.advance()?; // Skip 'function'
            
            let name = self.parse_name()?;
            
            let func = self.parse_function_body()?;
            
            Ok(Statement::LocalFunction {
                name,
                func,
            })
        } else {
            let mut names = vec![self.parse_name()?];
            
            // Parse additional names
            while self.check(&Token::Comma) {
                self.advance()?; // Skip ','
                names.push(self.parse_name()?);
            }
            
            // Optional initializers
            let values = if self.check(&Token::Assign) {
                self.advance()?; // Skip '='
                self.parse_expression_list()?
            } else {
                Vec::new()
            };
            
            Ok(Statement::LocalAssignment {
                names,
                values,
            })
        }
    }
    
    /// Parse a function statement
    fn parse_function_statement(&mut self) -> Result<Statement> {
        self.advance()?; // Skip 'function'
        
        let mut name_parts = vec![self.parse_name()?];
        let mut method_name = None;
        
        // Parse table field access chain
        while self.check(&Token::Dot) {
            self.advance()?; // Skip '.'
            name_parts.push(self.parse_name()?);
        }
        
        // Check for method syntax
        if self.check(&Token::Colon) {
            self.advance()?; // Skip ':'
            method_name = Some(self.parse_name()?);
        }
        
        // Create function name
        let base = name_parts[0].clone();
        let fields = name_parts[1..].to_vec();
        let name = FunctionName {
            base,
            fields,
            method: method_name,
        };
        
        // Parse function body
        let func = self.parse_function_body()?;
        
        Ok(Statement::Function(FunctionStatement {
            name,
            func,
        }))
    }
    
    /// Parse a function body (parameters and block)
    fn parse_function_body(&mut self) -> Result<FunctionDefinition> {
        self.expect(Token::LeftParen, "expected '(' for function parameters")?;
        
        let mut parameters = Vec::new();
        let mut is_variadic = false;
        
        // Parse parameters
        if !self.check(&Token::RightParen) {
            if self.check(&Token::Dots) {
                is_variadic = true;
                self.advance()?; // Skip '...'
            } else {
                parameters.push(self.parse_name()?);
                
                while self.check(&Token::Comma) {
                    self.advance()?; // Skip ','
                    
                    if self.check(&Token::Dots) {
                        is_variadic = true;
                        self.advance()?; // Skip '...'
                        break;
                    }
                    
                    parameters.push(self.parse_name()?);
                }
            }
        }
        
        self.expect(Token::RightParen, "expected ')' after function parameters")?;
        
        // Parse function body
        let body = self.parse_block()?;
        
        self.expect(Token::End, "expected 'end' to close function")?;
        
        Ok(FunctionDefinition {
            parameters,
            is_variadic,
            body,
        })
    }
    
    /// Parse a do statement
    fn parse_do_statement(&mut self) -> Result<Statement> {
        self.advance()?; // Skip 'do'
        
        let block = self.parse_block()?;
        
        self.expect(Token::End, "expected 'end' to close do block")?;
        
        Ok(Statement::Do(Box::new(block)))
    }
    
    /// Parse a return statement
    fn parse_return_statement(&mut self) -> Result<ReturnStatement> {
        self.advance()?; // Skip 'return'
        
        // Empty return if at block end or semicolon
        let values = if self.is_block_terminator() || self.check(&Token::Semicolon) {
            Vec::new()
        } else {
            self.parse_expression_list()?
        };
        
        Ok(ReturnStatement { values })
    }
    
    /// Parse a prefix expression (variable, function call, or parenthesized expression)
    fn parse_prefix_exp(&mut self) -> Result<Expression> {
        match &self.current {
            Token::Identifier(name) => {
                let name = name.clone();
                self.advance()?;
                
                // Continue parsing suffixes (field access, method call, function call)
                self.parse_prefix_exp_suffix(Expression::Variable(Variable::Name(name)))
            },
            Token::LeftParen => {
                self.advance()?; // Skip '('
                let exp = self.parse_expression()?;
                self.expect(Token::RightParen, "expected ')' after expression")?;
                
                // Continue parsing suffixes
                self.parse_prefix_exp_suffix(exp)
            },
            _ => Err(LuaError::Syntax(format!("unexpected token in expression: {:?}", self.current))),
        }
    }
    
    /// Parse suffix of a prefix expression (field access, method call, function call)
    fn parse_prefix_exp_suffix(&mut self, expr: Expression) -> Result<Expression> {
        match self.current {
            Token::LeftBracket => {
                // Table indexing with []
                self.advance()?; // Skip '['
                let key = self.parse_expression()?;
                self.expect(Token::RightBracket, "expected ']' after table key")?;
                
                // Create field access expression
                let var = match expr {
                    Expression::Variable(v) => v,
                    _ => return Err(LuaError::Syntax("expected variable before table index".to_string())),
                };
                
                let field_expr = Expression::Variable(Variable::Field {
                    table: Box::new(Expression::Variable(var)),
                    key: Box::new(key),
                });
                
                // Continue parsing suffixes
                self.parse_prefix_exp_suffix(field_expr)
            },
            Token::Dot => {
                // Table field access with .
                self.advance()?; // Skip '.'
                let field_name = self.parse_name()?;
                
                // Create field access expression
                let var = match expr {
                    Expression::Variable(v) => v,
                    _ => return Err(LuaError::Syntax("expected variable before field access".to_string())),
                };
                
                let field_expr = Expression::Variable(Variable::Field {
                    table: Box::new(Expression::Variable(var)),
                    key: Box::new(Expression::String(LuaString::from_str(&field_name))),
                });
                
                // Continue parsing suffixes
                self.parse_prefix_exp_suffix(field_expr)
            },
            Token::Colon => {
                // Method call (obj:method(...))
                self.advance()?; // Skip ':'
                let method_name = self.parse_name()?;
                
                // Parse arguments (required for method calls)
                self.expect(Token::LeftParen, "expected '(' after method name")?;
                let args = if self.check(&Token::RightParen) {
                    Vec::new()
                } else {
                    self.parse_expression_list()?
                };
                self.expect(Token::RightParen, "expected ')' after method arguments")?;
                
                // Create method call expression
                let func_call = FunctionCall {
                    func: Box::new(expr),
                    args,
                    is_method_call: true,
                    method_name: Some(method_name),
                };
                
                Ok(Expression::FunctionCall(func_call))
            },
            Token::LeftParen | Token::LeftBrace | Token::String(_) => {
                // Function call
                let args = self.parse_function_args()?;
                
                // Create function call expression
                let func_call = FunctionCall {
                    func: Box::new(expr),
                    args,
                    is_method_call: false,
                    method_name: None,
                };
                
                Ok(Expression::FunctionCall(func_call))
            },
            _ => {
                // No suffix, return the expression as is
                match expr {
                    Expression::Variable(_) | Expression::FunctionCall(_) => Ok(expr),
                    _ => Ok(Expression::Variable(Variable::Name("ERROR".to_string()))), // Should not happen
                }
            }
        }
    }
    
    /// Parse function arguments
    fn parse_function_args(&mut self) -> Result<Vec<Expression>> {
        match self.current {
            Token::LeftParen => {
                self.advance()?; // Skip '('
                
                let args = if self.check(&Token::RightParen) {
                    Vec::new()
                } else {
                    self.parse_expression_list()?
                };
                
                self.expect(Token::RightParen, "expected ')' after function arguments")?;
                
                Ok(args)
            },
            Token::LeftBrace => {
                // Table constructor as single argument
                let table = self.parse_table_constructor()?;
                Ok(vec![table])
            },
            Token::String(_) => {
                // String literal as single argument
                let string = self.parse_string_literal()?;
                Ok(vec![string])
            },
            _ => Err(LuaError::Syntax("expected function arguments".to_string())),
        }
    }
    
    /// Parse a list of expressions
    fn parse_expression_list(&mut self) -> Result<Vec<Expression>> {
        let mut expressions = vec![self.parse_expression()?];
        
        while self.check(&Token::Comma) {
            self.advance()?; // Skip ','
            expressions.push(self.parse_expression()?);
        }
        
        Ok(expressions)
    }
    
    /// Parse a single expression
    fn parse_expression(&mut self) -> Result<Expression> {
        self.parse_or_expression()
    }
    
    /// Parse an OR expression
    fn parse_or_expression(&mut self) -> Result<Expression> {
        let mut expr = self.parse_and_expression()?;
        
        while self.check(&Token::Or) {
            self.advance()?; // Skip 'or'
            let right = self.parse_and_expression()?;
            
            expr = Expression::BinaryOp {
                op: BinaryOp::Or,
                left: Box::new(expr),
                right: Box::new(right),
            };
        }
        
        Ok(expr)
    }
    
    /// Parse an AND expression
    fn parse_and_expression(&mut self) -> Result<Expression> {
        let mut expr = self.parse_comparison()?;
        
        while self.check(&Token::And) {
            self.advance()?; // Skip 'and'
            let right = self.parse_comparison()?;
            
            expr = Expression::BinaryOp {
                op: BinaryOp::And,
                left: Box::new(expr),
                right: Box::new(right),
            };
        }
        
        Ok(expr)
    }
    
    /// Parse a comparison expression
    fn parse_comparison(&mut self) -> Result<Expression> {
        let mut expr = self.parse_concat()?;
        
        // Parse comparison operators
        loop {
            let op = match self.current {
                Token::Less => BinaryOp::Less,
                Token::LessEqual => BinaryOp::LessEqual,
                Token::Greater => BinaryOp::Greater,
                Token::GreaterEqual => BinaryOp::GreaterEqual,
                Token::Equal => BinaryOp::Eq,
                Token::NotEqual => BinaryOp::NotEqual,
                _ => break,
            };
            
            self.advance()?;
            let right = self.parse_concat()?;
            
            expr = Expression::BinaryOp {
                op,
                left: Box::new(expr),
                right: Box::new(right),
            };
        }
        
        Ok(expr)
    }
    
    /// Parse a string concatenation expression
    fn parse_concat(&mut self) -> Result<Expression> {
        let mut expr = self.parse_addition()?;
        
        while self.check(&Token::Concat) {
            self.advance()?; // Skip '..'
            let right = self.parse_addition()?;
            
            expr = Expression::BinaryOp {
                op: BinaryOp::Concat,
                left: Box::new(expr),
                right: Box::new(right),
            };
        }
        
        Ok(expr)
    }
    
    /// Parse an addition/subtraction expression
    fn parse_addition(&mut self) -> Result<Expression> {
        let mut expr = self.parse_multiplication()?;
        
        loop {
            let op = match self.current {
                Token::Plus => BinaryOp::Add,
                Token::Minus => BinaryOp::Sub,
                _ => break,
            };
            
            self.advance()?;
            let right = self.parse_multiplication()?;
            
            expr = Expression::BinaryOp {
                op,
                left: Box::new(expr),
                right: Box::new(right),
            };
        }
        
        Ok(expr)
    }
    
    /// Parse a multiplication/division expression
    fn parse_multiplication(&mut self) -> Result<Expression> {
        let mut expr = self.parse_unary()?;
        
        loop {
            let op = match self.current {
                Token::Star => BinaryOp::Mul,
                Token::Slash => BinaryOp::Div,
                Token::Percent => BinaryOp::Mod,
                _ => break,
            };
            
            self.advance()?;
            let right = self.parse_unary()?;
            
            expr = Expression::BinaryOp {
                op,
                left: Box::new(expr),
                right: Box::new(right),
            };
        }
        
        Ok(expr)
    }
    
    /// Parse a unary expression
    fn parse_unary(&mut self) -> Result<Expression> {
        match self.current {
            Token::Minus => {
                self.advance()?;
                let operand = self.parse_unary()?;
                
                Ok(Expression::UnaryOp {
                    op: UnaryOp::Neg,
                    operand: Box::new(operand),
                })
            },
            Token::Not => {
                self.advance()?;
                let operand = self.parse_unary()?;
                
                Ok(Expression::UnaryOp {
                    op: UnaryOp::Not,
                    operand: Box::new(operand),
                })
            },
            Token::Hash => {
                self.advance()?;
                let operand = self.parse_unary()?;
                
                Ok(Expression::UnaryOp {
                    op: UnaryOp::Len,
                    operand: Box::new(operand),
                })
            },
            _ => self.parse_power(),
        }
    }
    
    /// Parse a power expression
    fn parse_power(&mut self) -> Result<Expression> {
        let mut expr = self.parse_primary()?;
        
        if self.check(&Token::Caret) {
            self.advance()?; // Skip '^'
            let right = self.parse_unary()?;
            
            expr = Expression::BinaryOp {
                op: BinaryOp::Pow,
                left: Box::new(expr),
                right: Box::new(right),
            };
        }
        
        Ok(expr)
    }
    
    /// Parse a primary expression
    fn parse_primary(&mut self) -> Result<Expression> {
        match &self.current {
            Token::Nil => {
                self.advance()?;
                Ok(Expression::Nil)
            },
            Token::True => {
                self.advance()?;
                Ok(Expression::Boolean(true))
            },
            Token::False => {
                self.advance()?;
                Ok(Expression::Boolean(false))
            },
            Token::Number(n) => {
                let value = *n;
                self.advance()?;
                Ok(Expression::Number(value))
            },
            Token::String(_) => self.parse_string_literal(),
            Token::Dots => {
                self.advance()?;
                Ok(Expression::Vararg)
            },
            Token::LeftBrace => self.parse_table_constructor(),
            Token::Function => {
                self.advance()?; // Skip 'function'
                let func = self.parse_function_body()?;
                Ok(Expression::Function(func))
            },
            // Prefix expression (variable, function call, or parenthesized expression)
            _ => self.parse_prefix_exp(),
        }
    }
    
    /// Parse a string literal
    fn parse_string_literal(&mut self) -> Result<Expression> {
        if let Token::String(bytes) = &self.current {
            let bytes = bytes.clone();
            self.advance()?;
            Ok(Expression::String(LuaString::from_bytes(bytes)))
        } else {
            Err(LuaError::Syntax("expected string literal".to_string()))
        }
    }
    
    /// Parse a table constructor
    fn parse_table_constructor(&mut self) -> Result<Expression> {
        self.expect(Token::LeftBrace, "expected '{' for table constructor")?;
        
        let mut fields = Vec::new();
        
        if !self.check(&Token::RightBrace) {
            // Parse first field
            fields.push(self.parse_table_field()?);
            
            // Parse additional fields
            while self.check(&Token::Comma) || self.check(&Token::Semicolon) {
                self.advance()?; // Skip ',' or ';'
                
                if self.check(&Token::RightBrace) {
                    break;
                }
                
                fields.push(self.parse_table_field()?);
            }
        }
        
        self.expect(Token::RightBrace, "expected '}' to close table constructor")?;
        
        Ok(Expression::Table(fields))
    }
    
    /// Parse a table field
    fn parse_table_field(&mut self) -> Result<TableField> {
        if self.check(&Token::LeftBracket) {
            // Explicit key
            self.advance()?; // Skip '['
            let key = self.parse_expression()?;
            self.expect(Token::RightBracket, "expected ']' after table key")?;
            self.expect(Token::Assign, "expected '=' after table key")?;
            let value = self.parse_expression()?;
            
            Ok(TableField::KeyValue { key, value })
        } else if let Token::Identifier(name) = &self.current {
            let name = name.clone();
            self.advance()?;
            
            if self.check(&Token::Assign) {
                // Named field
                self.advance()?; // Skip '='
                let value = self.parse_expression()?;
                
                Ok(TableField::NamedField { name, value })
            } else {
                // Simple value (array part)
                Ok(TableField::Value(Expression::Variable(Variable::Name(name))))
            }
        } else {
            // Simple value (array part)
            let value = self.parse_expression()?;
            Ok(TableField::Value(value))
        }
    }
    
    /// Parse a variable name
    fn parse_name(&mut self) -> Result<String> {
        if let Token::Identifier(name) = &self.current {
            let name = name.clone();
            self.advance()?;
            Ok(name)
        } else {
            Err(LuaError::Syntax(format!("expected identifier, got {:?}", self.current)))
        }
    }
    
    /// Advance to the next token
    fn advance(&mut self) -> Result<()> {
        match self.lexer.next() {
            Some(Ok((token, position))) => {
                self.current = token;
                self.position = position;
                Ok(())
            },
            Some(Err(e)) => Err(e),
            None => {
                self.current = Token::Eof;
                Ok(())
            }
        }
    }
    
    /// Check if the current token matches the expected token
    fn check(&self, expected: &Token) -> bool {
        match (&self.current, expected) {
            (Token::Identifier(a), Token::Identifier(b)) => a == b,
            (a, b) => std::mem::discriminant(a) == std::mem::discriminant(b),
        }
    }
    
    /// Expect a token and advance if it matches, otherwise error
    fn expect(&mut self, expected: Token, error_msg: &str) -> Result<()> {
        if self.check(&expected) {
            self.advance()
        } else {
            Err(LuaError::Syntax(format!("{} at {}", error_msg, self.position)))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_parse_simple_expression() {
        let mut parser = Parser::new("1 + 2 * 3").unwrap();
        let expr = parser.parse_expression().unwrap();
        
        // Should parse as 1 + (2 * 3) due to operator precedence
        match expr {
            Expression::BinaryOp { op: BinaryOp::Add, .. } => {
                // Correctly parsed addition as top-level
            },
            _ => panic!("Expected BinaryOp::Add, got {:?}", expr),
        }
    }
    
    #[test]
    fn test_parse_simple_assignment() {
        let mut parser = Parser::new("x = 10").unwrap();
        let chunk = parser.parse().unwrap();
        
        assert_eq!(chunk.block.statements.len(), 1);
        match &chunk.block.statements[0] {
            Statement::Assignment(assign) => {
                assert_eq!(assign.vars.len(), 1);
                assert_eq!(assign.values.len(), 1);
                
                match &assign.vars[0] {
                    Variable::Name(name) => assert_eq!(name, "x"),
                    _ => panic!("Expected Variable::Name"),
                }
                
                match &assign.values[0] {
                    Expression::Number(n) => assert_eq!(*n, 10.0),
                    _ => panic!("Expected Expression::Number"),
                }
            },
            _ => panic!("Expected Statement::Assignment"),
        }
    }
    
    #[test]
    fn test_parse_if_statement() {
        let mut parser = Parser::new("if x > 0 then print(x) end").unwrap();
        let chunk = parser.parse().unwrap();
        
        assert_eq!(chunk.block.statements.len(), 1);
        match &chunk.block.statements[0] {
            Statement::If(if_stmt) => {
                match &if_stmt.condition {
                    Expression::BinaryOp { op: BinaryOp::Greater, .. } => {
                        // Correctly parsed comparison
                    },
                    _ => panic!("Expected BinaryOp::Greater"),
                }
                
                assert_eq!(if_stmt.then_block.statements.len(), 1);
                assert!(if_stmt.elseif_branches.is_empty());
                assert!(if_stmt.else_block.is_none());
            },
            _ => panic!("Expected Statement::If"),
        }
    }
}