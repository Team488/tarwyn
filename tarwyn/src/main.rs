use log::info;
use tarwyn_server::{utils::log::init_logger, tarwyn_server::TarwynServer};

//simple usage of using tarwyn server and tarwyn client
#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    init_logger();

    let tarwyn_server = TarwynServer::new();
    tarwyn_server.start();

    info!("Tarwyn server started successfully.");

    // Prevent main from exiting
    loop {
        // Here you can add logic to interact with the server or handle other tasks
        // For demonstration, we will just sleep for a while
        tokio::time::sleep(tokio::time::Duration::from_secs(5)).await;
    }
}
