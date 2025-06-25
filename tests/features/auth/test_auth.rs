// Test authentication feature

use ferrous::network::{NetworkConfig, Server};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("Starting Ferrous server with password authentication...");

    // Create custom config with a password
    let mut config = NetworkConfig::default();
    config.password = Some("mysecretpassword".to_string());

    // Create and run server
    let mut server = Server::with_config(config)?;

    println!("Server ready! Password is 'mysecretpassword'");
    println!("Use AUTH command to authenticate before using other commands");

    server.run()
}
