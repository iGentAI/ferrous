//! Simple tests for the Lua lexer and parser

use crate::lua::lexer::{Lexer, Token};
use std::fmt;
use std::error::Error;

type TestResult = Result<(), Box<dyn Error>>;

#[derive(Debug)]
struct TestError(String);

impl Error for TestError {}

impl fmt::Display for TestError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

pub fn run_simple_tests() -> bool {
    let mut success = true;
    
    // Test 1: Lexer test
    println!("Running simple lexer test...");
    if let Err(e) = test_lexer() {
        println!("  ❌ Lexer test failed: {}", e);
        success = false;
    } else {
        println!("  ✅ Lexer test passed!");
    }
    
    success
}

fn test_lexer() -> TestResult {
    let input = "local x = 10";
    let mut lexer = Lexer::new(input);
    
    // First token should be 'local'
    match lexer.next_token() {
        Ok(Token::Local) => {},
        Ok(other) => return Err(Box::new(TestError(format!("Expected Token::Local, got {:?}", other)))),
        Err(e) => return Err(Box::new(TestError(format!("Lexer error: {}", e)))),
    }
    
    // Second token should be identifier 'x'
    match lexer.next_token() {
        Ok(Token::Identifier(name)) if name == "x" => {},
        Ok(other) => return Err(Box::new(TestError(format!("Expected identifier 'x', got {:?}", other)))),
        Err(e) => return Err(Box::new(TestError(format!("Lexer error: {}", e)))),
    }
    
    // Third token should be '='
    match lexer.next_token() {
        Ok(Token::Assign) => {},
        Ok(other) => return Err(Box::new(TestError(format!("Expected Token::Assign, got {:?}", other)))),
        Err(e) => return Err(Box::new(TestError(format!("Lexer error: {}", e)))),
    }
    
    // Fourth token should be number 10
    match lexer.next_token() {
        Ok(Token::Number(value)) if value == 10.0 => {},
        Ok(other) => return Err(Box::new(TestError(format!("Expected number 10, got {:?}", other)))),
        Err(e) => return Err(Box::new(TestError(format!("Lexer error: {}", e)))),
    }
    
    // Last token should be EOF
    match lexer.next_token() {
        Ok(Token::Eof) => {},
        Ok(other) => return Err(Box::new(TestError(format!("Expected Token::Eof, got {:?}", other)))),
        Err(e) => return Err(Box::new(TestError(format!("Lexer error: {}", e)))),
    }
    
    Ok(())
}