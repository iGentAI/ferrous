//! Command-line argument parser
//!
//! Parses command-line arguments for Ferrous, with Redis compatibility.

use std::path::PathBuf;

/// Command-line arguments for Ferrous
#[derive(Debug, Clone)]
pub struct CliArgs {
    /// Configuration file path
    pub config: Option<PathBuf>,
    
    /// Port to listen on
    pub port: Option<u16>,
    
    /// Address to bind to 
    pub bind: Option<String>,
    
    /// Password for authentication
    pub password: Option<String>,
    
    /// Master to replicate from - (host, port)
    pub replicaof: Option<(String, u16)>,
    
    /// Directory for data files
    pub dir: Option<String>,
    
    /// Database filename
    pub dbfilename: Option<String>,
    
    /// Enable AOF
    pub appendonly: bool,
    
    /// Logfile path
    pub logfile: Option<String>,
    
    /// Log level (debug, verbose, notice, warning)
    pub loglevel: Option<String>,
    
    /// Whether to fork and run in the background
    pub daemonize: Option<bool>,
}

impl Default for CliArgs {
    fn default() -> Self {
        CliArgs {
            config: None,
            port: None,
            bind: None,
            password: None,
            replicaof: None,
            dir: None,
            dbfilename: None,
            appendonly: false,
            logfile: None,
            loglevel: None,
            daemonize: None,
        }
    }
}

/// Parse command-line arguments
pub fn parse_cli_args() -> CliArgs {
    // Get args without program name
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.is_empty() {
        return CliArgs::default();
    }
    
    let mut cli_args = CliArgs::default();
    let mut i = 0;
    
    while i < args.len() {
        match args[i].as_str() {
            "--help" | "-h" => {
                print_help();
                std::process::exit(0);
            }
            "--version" | "-v" => {
                println!("Ferrous {}", env!("CARGO_PKG_VERSION"));
                std::process::exit(0);
            }
            "--config" | "-c" => {
                if i + 1 < args.len() {
                    cli_args.config = Some(PathBuf::from(&args[i + 1]));
                    i += 2;
                } else {
                    eprintln!("Error: Missing argument for --config");
                    std::process::exit(1);
                }
            }
            "--port" | "-p" => {
                if i + 1 < args.len() {
                    match args[i + 1].parse::<u16>() {
                        Ok(port) => cli_args.port = Some(port),
                        Err(_) => {
                            eprintln!("Error: Invalid port number: {}", args[i + 1]);
                            std::process::exit(1);
                        }
                    }
                    i += 2;
                } else {
                    eprintln!("Error: Missing argument for --port");
                    std::process::exit(1);
                }
            }
            "--bind" => {
                if i + 1 < args.len() {
                    cli_args.bind = Some(args[i + 1].clone());
                    i += 2;
                } else {
                    eprintln!("Error: Missing argument for --bind");
                    std::process::exit(1);
                }
            }
            "--password" | "--requirepass" => {
                if i + 1 < args.len() {
                    cli_args.password = Some(args[i + 1].clone());
                    i += 2;
                } else {
                    eprintln!("Error: Missing argument for --password");
                    std::process::exit(1);
                }
            }
            "--replicaof" | "--slaveof" => {
                if i + 2 < args.len() {
                    match args[i + 2].parse::<u16>() {
                        Ok(port) => cli_args.replicaof = Some((args[i + 1].clone(), port)),
                        Err(_) => {
                            eprintln!("Error: Invalid port number for --replicaof: {}", args[i + 2]);
                            std::process::exit(1);
                        }
                    }
                    i += 3;
                } else {
                    eprintln!("Error: Missing arguments for --replicaof");
                    std::process::exit(1);
                }
            }
            "--dir" => {
                if i + 1 < args.len() {
                    cli_args.dir = Some(args[i + 1].clone());
                    i += 2;
                } else {
                    eprintln!("Error: Missing argument for --dir");
                    std::process::exit(1);
                }
            }
            "--dbfilename" => {
                if i + 1 < args.len() {
                    cli_args.dbfilename = Some(args[i + 1].clone());
                    i += 2;
                } else {
                    eprintln!("Error: Missing argument for --dbfilename");
                    std::process::exit(1);
                }
            }
            "--appendonly" => {
                if i + 1 < args.len() && (args[i + 1] == "yes" || args[i + 1] == "no") {
                    cli_args.appendonly = args[i + 1] == "yes";
                    i += 2;
                } else {
                    // Just --appendonly with no argument means enable it
                    cli_args.appendonly = true;
                    i += 1;
                }
            }
            "--logfile" => {
                if i + 1 < args.len() {
                    cli_args.logfile = Some(args[i + 1].clone());
                    i += 2;
                } else {
                    eprintln!("Error: Missing argument for --logfile");
                    std::process::exit(1);
                }
            }
            "--loglevel" => {
                if i + 1 < args.len() {
                    cli_args.loglevel = Some(args[i + 1].clone());
                    i += 2;
                } else {
                    eprintln!("Error: Missing argument for --loglevel");
                    std::process::exit(1);
                }
            }
            "--daemonize" => {
                if i + 1 < args.len() && (args[i + 1] == "yes" || args[i + 1] == "no") {
                    cli_args.daemonize = Some(args[i + 1] == "yes");
                    i += 2;
                } else {
                    // Just --daemonize with no argument means enable it
                    cli_args.daemonize = Some(true);
                    i += 1;
                }
            }
            arg => {
                // Check if it's a config file path without --config flag
                if arg.ends_with(".conf") {
                    cli_args.config = Some(PathBuf::from(arg));
                    i += 1;
                } else {
                    eprintln!("Error: Unknown argument: {}", arg);
                    print_help();
                    std::process::exit(1);
                }
            }
        }
    }
    
    cli_args
}

/// Print help information
fn print_help() {
    println!("Usage: ferrous [OPTIONS] [/path/to/ferrous.conf]");
    println!("       ferrous --port 6379");
    println!("       ferrous /etc/ferrous.conf --loglevel debug");
    println!();
    println!("Options:");
    println!("  --help, -h              Show this help message");
    println!("  --version, -v           Show version information");
    println!("  --config, -c  <file>    Configuration file to use");
    println!("  --port, -p    <port>    TCP port to listen on (default: 6379)");
    println!("  --bind        <address> Interface to bind to (default: 127.0.0.1)");
    println!("  --password    <password> Server password");
    println!("  --replicaof   <host> <port> Make this server a replica of another instance");
    println!("  --dir         <dir>     Working directory for database files");
    println!("  --dbfilename  <filename> Database filename");
    println!("  --appendonly  [yes|no]  Enable append-only file persistence");
    println!("  --logfile     <file>    Path to log file (empty for stdout)");
    println!("  --loglevel    <level>   Log level (debug, verbose, notice, warning)");
    println!("  --daemonize   [yes|no]  Run as a daemon in the background");
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_cli_args() {
        // Test with args
        let mut args = CliArgs::default();
        args.port = Some(9999);
        args.password = Some("secret".to_string());
        
        assert_eq!(args.port, Some(9999));
        assert_eq!(args.password, Some("secret".to_string()));
        assert_eq!(args.replicaof, None);
        
        // Test with replicaof
        let mut args = CliArgs::default();
        args.replicaof = Some(("master.example.com".to_string(), 6379));
        
        assert_eq!(args.replicaof, Some(("master.example.com".to_string(), 6379)));
    }
}