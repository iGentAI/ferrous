//! Test program for the Lua lexer
//! 
//! This is a simple standalone test that only uses the lexer component.

use ferrous::lua::lexer::{Lexer, Token};

fn main() {
    println!("--- Ferrous Lua Lexer Test ---\n");
    
    let input = r#"
        -- This is a test Lua script
        local x = 10
        local y = 20 
        
        function add(a, b)
            return a + b
        end
        
        print(add(x, y))  -- Should print 30
    "#;
    
    println!("Test script:");
    println!("-----------");
    println!("{}", input);
    println!("-----------\n");
    
    println!("Tokenizing...\n");
    
    let mut lexer = Lexer::new(input);
    let mut token_count = 0;
    
    // Process all tokens
    loop {
        match lexer.next_token() {
            Ok(Token::Eof) => {
                println!("#{}: EOF", token_count + 1);
                break;
            },
            Ok(token) => {
                token_count += 1;
                println!("#{}: {:?}", token_count, token);
            },
            Err(e) => {
                println!("Error: {}", e);
                std::process::exit(1);
            }
        }
    }
    
    println!("\nSuccessfully parsed {} tokens!", token_count);
}