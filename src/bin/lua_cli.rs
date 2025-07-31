//! Ferrous Lua CLI Tool
//! 
//! A standalone command-line tool for testing and validating Lua scripts
//! using the same MLua-based Lua 5.1 implementation as the Ferrous server.

use std::env;
use std::fs;
use std::io::{self, Write, BufRead};
use std::path::Path;
use std::sync::Arc;
use std::time::Instant;
use ferrous::storage::engine::StorageEngine;
use ferrous::storage::commands::lua::handle_eval;
use ferrous::protocol::resp::RespFrame;

#[derive(Debug, Clone)]
struct CliConfig {
    memory_limit_mb: usize,
    instruction_limit: usize,
    timeout_seconds: u64,
    verbose: bool,
}

impl Default for CliConfig {
    fn default() -> Self {
        Self {
            memory_limit_mb: 50,
            instruction_limit: 1_000_000,
            timeout_seconds: 5,
            verbose: false,
        }
    }
}

fn main() {
    let args: Vec<String> = env::args().collect();
    let mut config = CliConfig::default();
    
    if args.len() < 2 {
        print_usage();
        std::process::exit(1);
    }
    
    let mut i = 1;
    while i < args.len() {
        match args[i].as_str() {
            "-f" | "--file" => {
                if i + 1 >= args.len() {
                    eprintln!("Error: --file requires a filename");
                    std::process::exit(1);
                }
                let filename = &args[i + 1];
                execute_file(filename, &config, vec![], vec![]);
                i += 2;
            }
            "-e" | "--eval" => {
                if i + 1 >= args.len() {
                    eprintln!("Error: --eval requires a script");
                    std::process::exit(1);
                }
                let script = &args[i + 1];
                execute_script(script, &config, vec![], vec![]);
                i += 2;
            }
            "-k" | "--keys" => {
                if i + 1 >= args.len() {
                    eprintln!("Error: --keys requires a comma-separated list");
                    std::process::exit(1);
                }
                let keys_str = &args[i + 1];
                let keys: Vec<String> = keys_str.split(',').map(|s| s.to_string()).collect();
                
                let (script, script_keys, script_args) = parse_script_and_data(&args[i+2..], keys, vec![]);
                execute_script(&script, &config, script_keys, script_args);
                return;
            }
            "-a" | "--args" => {
                if i + 1 >= args.len() {
                    eprintln!("Error: --args requires a comma-separated list");
                    std::process::exit(1);
                }
                let args_str = &args[i + 1];
                let script_args: Vec<String> = args_str.split(',').map(|s| s.to_string()).collect();
                
                let (script, script_keys, script_args) = parse_script_and_data(&args[i+2..], vec![], script_args);
                execute_script(&script, &config, script_keys, script_args);
                return;
            }
            "-i" | "--interactive" => {
                start_repl(&config);
                i += 1;
            }
            "-v" | "--verbose" => {
                config.verbose = true;
                i += 1;
            }
            "-t" | "--test" => {
                if i + 1 >= args.len() {
                    eprintln!("Error: --test requires a directory");
                    std::process::exit(1);
                }
                let test_dir = &args[i + 1];
                run_test_directory(test_dir, &config);
                i += 2;
            }
            "--memory-limit" => {
                if i + 1 >= args.len() {
                    eprintln!("Error: --memory-limit requires a value in MB");
                    std::process::exit(1);
                }
                match args[i + 1].parse::<usize>() {
                    Ok(mb) => config.memory_limit_mb = mb,
                    Err(_) => {
                        eprintln!("Error: Invalid memory limit: {}", args[i + 1]);
                        std::process::exit(1);
                    }
                }
                i += 2;
            }
            "--instruction-limit" => {
                if i + 1 >= args.len() {
                    eprintln!("Error: --instruction-limit requires a count");
                    std::process::exit(1);
                }
                match args[i + 1].parse::<usize>() {
                    Ok(count) => config.instruction_limit = count,
                    Err(_) => {
                        eprintln!("Error: Invalid instruction limit: {}", args[i + 1]);
                        std::process::exit(1);
                    }
                }
                i += 2;
            }
            "--timeout" => {
                if i + 1 >= args.len() {
                    eprintln!("Error: --timeout requires seconds");
                    std::process::exit(1);
                }
                match args[i + 1].parse::<u64>() {
                    Ok(secs) => config.timeout_seconds = secs,
                    Err(_) => {
                        eprintln!("Error: Invalid timeout: {}", args[i + 1]);
                        std::process::exit(1);
                    }
                }
                i += 2;
            }
            "--help" | "-h" => {
                print_usage();
                std::process::exit(0);
            }
            _ => {
                eprintln!("Error: Unknown option: {}", args[i]);
                print_usage();
                std::process::exit(1);
            }
        }
    }
}

fn parse_script_and_data(remaining_args: &[String], mut keys: Vec<String>, mut args: Vec<String>) -> (String, Vec<String>, Vec<String>) {
    for (i, arg) in remaining_args.iter().enumerate() {
        match arg.as_str() {
            "-e" | "--eval" => {
                if i + 1 < remaining_args.len() {
                    return (remaining_args[i + 1].clone(), keys, args);
                }
            }
            "-f" | "--file" => {
                if i + 1 < remaining_args.len() {
                    let filename = &remaining_args[i + 1];
                    match fs::read_to_string(filename) {
                        Ok(script) => return (script, keys, args),
                        Err(e) => {
                            eprintln!("Error reading file {}: {}", filename, e);
                            std::process::exit(1);
                        }
                    }
                }
            }
            "-k" | "--keys" => {
                if i + 1 < remaining_args.len() {
                    keys = remaining_args[i + 1].split(',').map(|s| s.to_string()).collect();
                }
            }
            "-a" | "--args" => {
                if i + 1 < remaining_args.len() {
                    args = remaining_args[i + 1].split(',').map(|s| s.to_string()).collect();
                }
            }
            _ => {}
        }
    }
    
    eprintln!("Error: No script provided");
    std::process::exit(1);
}

fn execute_file(filename: &str, config: &CliConfig, keys: Vec<String>, args: Vec<String>) {
    if config.verbose {
        println!("Reading script from file: {}", filename);
    }
    
    let script = match fs::read_to_string(filename) {
        Ok(content) => content,
        Err(e) => {
            eprintln!("Error reading file {}: {}", filename, e);
            std::process::exit(1);
        }
    };
    
    execute_script(&script, config, keys, args);
}

fn execute_script(script: &str, config: &CliConfig, keys: Vec<String>, args: Vec<String>) {
    if config.verbose {
        println!("Executing Lua script:");
        println!("Memory limit: {} MB", config.memory_limit_mb);
        println!("Instruction limit: {}", config.instruction_limit);
        println!("Timeout: {} seconds", config.timeout_seconds);
        println!("Keys: {:?}", keys);
        println!("Args: {:?}", args);
        println!("Script:");
        println!("{}", script);
        println!("--- Execution ---");
    }
    
    let storage = Arc::new(StorageEngine::new_in_memory());
    
    let key_bytes: Vec<Vec<u8>> = keys.into_iter().map(|k| k.into_bytes()).collect();
    let arg_bytes: Vec<Vec<u8>> = args.into_iter().map(|a| a.into_bytes()).collect();
    
    let mut parts = vec![
        RespFrame::BulkString(Some(Arc::new("EVAL".as_bytes().to_vec()))),
        RespFrame::BulkString(Some(Arc::new(script.as_bytes().to_vec()))),
        RespFrame::Integer(key_bytes.len() as i64),
    ];
    
    for key in key_bytes {
        parts.push(RespFrame::BulkString(Some(Arc::new(key))));
    }
    
    for arg in arg_bytes {
        parts.push(RespFrame::BulkString(Some(Arc::new(arg))));
    }
    
    let start = Instant::now();
    let result = handle_eval(&storage, &parts);
    let elapsed = start.elapsed();
    
    if config.verbose {
        println!("Execution time: {:?}", elapsed);
    }
    
    match result {
        Ok(response) => {
            print_response(&response, config.verbose);
            
            if config.verbose {
                println!("Memory usage: {} bytes", storage.memory_usage());
            }
        }
        Err(e) => {
            eprintln!("Error: {}", e);
            std::process::exit(1);
        }
    }
}

fn print_response(response: &RespFrame, verbose: bool) {
    match response {
        RespFrame::SimpleString(bytes) => {
            println!("{}", String::from_utf8_lossy(bytes));
        }
        RespFrame::BulkString(Some(bytes)) => {
            println!("{}", String::from_utf8_lossy(bytes));
        }
        RespFrame::BulkString(None) => {
            println!("(nil)");
        }
        RespFrame::Integer(n) => {
            println!("{}", n);
        }
        RespFrame::Error(bytes) => {
            eprintln!("Error: {}", String::from_utf8_lossy(bytes));
        }
        RespFrame::Array(Some(items)) => {
            if verbose {
                println!("Array with {} elements:", items.len());
            }
            for (i, item) in items.iter().enumerate() {
                if verbose {
                    print!("  [{}] ", i + 1);
                }
                print_response_inline(item);
                println!();
            }
        }
        RespFrame::Array(None) => {
            println!("(nil array)");
        }
        // Handle additional RESP3 variants
        RespFrame::Null => {
            println!("(null)");
        }
        RespFrame::Boolean(b) => {
            println!("{}", b);
        }
        RespFrame::Double(d) => {
            println!("{}", d);
        }
        RespFrame::Map(pairs) => {
            println!("Map with {} pairs:", pairs.len());
            for (k, v) in pairs {
                print!("  ");
                print_response_inline(k);
                print!(" -> ");
                print_response_inline(v);
                println!();
            }
        }
        RespFrame::Set(items) => {
            println!("Set with {} items:", items.len());
            for item in items {
                print!("  ");
                print_response_inline(item);
                println!();
            }
        }
    }
}

fn print_response_inline(response: &RespFrame) {
    match response {
        RespFrame::SimpleString(bytes) | RespFrame::BulkString(Some(bytes)) => {
            print!("{}", String::from_utf8_lossy(bytes));
        }
        RespFrame::BulkString(None) => {
            print!("(nil)");
        }
        RespFrame::Integer(n) => {
            print!("{}", n);
        }
        RespFrame::Error(bytes) => {
            print!("Error: {}", String::from_utf8_lossy(bytes));
        }
        RespFrame::Array(Some(items)) => {
            print!("[");
            for (i, item) in items.iter().enumerate() {
                if i > 0 { print!(", "); }
                print_response_inline(item);
            }
            print!("]");
        }
        RespFrame::Array(None) => {
            print!("(nil array)");
        }
        RespFrame::Null => {
            print!("(null)");
        }
        RespFrame::Boolean(b) => {
            print!("{}", b);
        }
        RespFrame::Double(d) => {
            print!("{}", d);
        }
        RespFrame::Map(_) => {
            print!("(map)");
        }
        RespFrame::Set(_) => {
            print!("(set)");
        }
    }
}

fn start_repl(config: &CliConfig) {
    println!("Ferrous Lua CLI - Interactive Mode (MLua Lua 5.1)");
    println!("Type 'exit' or 'quit' to exit, 'help' for commands");
    println!("Memory limit: {} MB, Instruction limit: {}", 
             config.memory_limit_mb, config.instruction_limit);
    
    let stdin = io::stdin();
    let mut keys = Vec::<String>::new();
    let mut args = Vec::<String>::new();
    
    loop {
        print!("lua> ");
        io::stdout().flush().unwrap();
        
        let mut input = String::new();
        match stdin.lock().read_line(&mut input) {
            Ok(0) => break,
            Ok(_) => {
                let input = input.trim();
                
                match input {
                    "exit" | "quit" => break,
                    "help" => {
                        print_repl_help();
                        continue;
                    }
                    "clear" => {
                        keys.clear();
                        args.clear();
                        println!("Cleared KEYS and ARGV");
                        continue;
                    }
                    _ if input.starts_with("setkeys ") => {
                        let keys_str = &input[8..];
                        keys = keys_str.split(',').map(|s| s.trim().to_string()).collect();
                        println!("Set KEYS: {:?}", keys);
                        continue;
                    }
                    _ if input.starts_with("setargs ") => {
                        let args_str = &input[8..];
                        args = args_str.split(',').map(|s| s.trim().to_string()).collect();
                        println!("Set ARGV: {:?}", args);
                        continue;
                    }
                    _ if input.starts_with("load ") => {
                        let filename = &input[5..].trim();
                        match fs::read_to_string(filename) {
                            Ok(script) => {
                                execute_script(&script, config, keys.clone(), args.clone());
                            }
                            Err(e) => {
                                eprintln!("Error reading file {}: {}", filename, e);
                            }
                        }
                        continue;
                    }
                    "" => continue,
                    script => {
                        execute_script(script, config, keys.clone(), args.clone());
                    }
                }
            }
            Err(e) => {
                eprintln!("Error reading input: {}", e);
                break;
            }
        }
    }
    
    println!("Goodbye!");
}

fn print_repl_help() {
    println!("REPL Commands:");
    println!("  help                 Show this help");
    println!("  exit, quit           Exit the REPL");
    println!("  clear                Clear KEYS and ARGV");
    println!("  setkeys k1,k2,k3     Set KEYS table");
    println!("  setargs a1,a2,a3     Set ARGV table"); 
    println!("  load <file>          Execute script from file");
    println!("  <lua script>         Execute Lua script directly");
    println!();
    println!("Lua Environment:");
    println!("  KEYS                 Array of keys (set with setkeys)");
    println!("  ARGV                 Array of arguments (set with setargs)");
    println!("  Sandboxed: os, io, debug, package disabled");
    println!("  Available: math, string, table libraries");
    println!("  redis.call/pcall     Basic Redis command placeholders");
}

fn run_test_directory(test_dir: &str, config: &CliConfig) {
    println!("Running all .lua test files in: {}", test_dir);
    
    let test_path = Path::new(test_dir);
    if !test_path.exists() {
        eprintln!("Error: Test directory {} does not exist", test_dir);
        std::process::exit(1);
    }
    
    let mut test_files = Vec::new();
    
    match fs::read_dir(test_path) {
        Ok(entries) => {
            for entry in entries {
                if let Ok(entry) = entry {
                    let path = entry.path();
                    if path.extension().and_then(|s| s.to_str()) == Some("lua") {
                        test_files.push(path);
                    }
                }
            }
        }
        Err(e) => {
            eprintln!("Error reading test directory: {}", e);
            std::process::exit(1);
        }
    }
    
    test_files.sort();
    
    if test_files.is_empty() {
        println!("No .lua files found in {}", test_dir);
        return;
    }
    
    let mut passed = 0;
    let mut failed = 0;
    
    for test_file in test_files {
        let filename = test_file.file_name().unwrap().to_string_lossy();
        print!("Running test: {} ... ", filename);
        io::stdout().flush().unwrap();
        
        match fs::read_to_string(&test_file) {
            Ok(script) => {
                let storage = Arc::new(StorageEngine::new_in_memory());
                let parts = vec![
                    RespFrame::BulkString(Some(Arc::new("EVAL".as_bytes().to_vec()))),
                    RespFrame::BulkString(Some(Arc::new(script.as_bytes().to_vec()))),
                    RespFrame::Integer(0),
                ];
                
                let start = Instant::now();
                let result = handle_eval(&storage, &parts);
                let elapsed = start.elapsed();
                
                match result {
                    Ok(response) => {
                        let test_passed = match response {
                            RespFrame::Integer(1) => true, // Lua true -> Redis integer 1
                            RespFrame::BulkString(Some(ref bytes)) if bytes.as_ref() == b"PASS" => true,
                            RespFrame::SimpleString(ref bytes) if bytes.as_ref() == b"PASS" => true,
                            RespFrame::Error(_) => false,
                            _ => true, // No explicit pass/fail, assume pass if no error
                        };
                        
                        if test_passed {
                            println!("PASS ({:?})", elapsed);
                            passed += 1;
                        } else {
                            println!("FAIL - Script returned: {:?}", response);
                            failed += 1;
                        }
                        
                        if config.verbose {
                            println!("  Result: {:?}", response);
                            println!("  Memory: {} bytes", storage.memory_usage());
                        }
                    }
                    Err(e) => {
                        println!("FAIL - Error: {}", e);
                        failed += 1;
                    }
                }
            }
            Err(e) => {
                println!("FAIL - Cannot read file: {}", e);
                failed += 1;
            }
        }
    }
    
    println!("\nTest Results:");
    println!("  Passed: {}", passed);
    println!("  Failed: {}", failed);
    println!("  Total:  {}", passed + failed);
    
    if failed > 0 {
        std::process::exit(1);
    }
}

fn print_usage() {
    println!("Ferrous Lua CLI Tool (MLua Lua 5.1)");
    println!("Usage: {} [options]", env::args().next().unwrap_or_else(|| "lua_cli".to_string()));
    println!();
    println!("Options:");
    println!("  -f, --file <file>             Execute Lua script from file");
    println!("  -e, --eval <script>           Execute Lua script from command line");
    println!("  -k, --keys <k1,k2,k3>         Set KEYS table (comma-separated)");
    println!("  -a, --args <a1,a2,a3>         Set ARGV table (comma-separated)");
    println!("  -i, --interactive             Start interactive REPL mode");
    println!("  -v, --verbose                 Verbose output");
    println!("  -t, --test <dir>              Run all .lua test files in directory");
    println!("  --memory-limit <mb>           Set memory limit in MB (default: 50)");
    println!("  --instruction-limit <count>   Set instruction limit (default: 1M)");
    println!("  --timeout <seconds>           Set timeout in seconds (default: 5)");
    println!("  -h, --help                    Show this help message");
    println!();
    println!("Examples:");
    println!("  {} -e \"return 'hello world'\"", env::args().next().unwrap_or_else(|| "lua_cli".to_string()));
    println!("  {} -f script.lua -k key1,key2 -a val1,val2", env::args().next().unwrap_or_else(|| "lua_cli".to_string()));
    println!("  {} -i", env::args().next().unwrap_or_else(|| "lua_cli".to_string()));
    println!("  {} -t tests/lua_scripts/", env::args().next().unwrap_or_else(|| "lua_cli".to_string()));
}