//! Simple test binary for the Lua interpreter lexer

extern crate ferrous;

fn main() {
    println!("--- Ferrous Lua Lexer Tests ---");
    
    if ferrous::lua::simple_test::run_simple_tests() {
        println!("\nAll simple tests passed!");
        std::process::exit(0);
    } else {
        println!("\nSome tests failed!");
        std::process::exit(1);
    }
}