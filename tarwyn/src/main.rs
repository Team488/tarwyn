use tarwyn_server::tarwyn_server::TarwynServer;

//simple usage of using tarwyn server and tarwyn client
#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let tarwyn_server = TarwynServer::new();
    tarwyn_server.start();
    println!("Tarwyn server started and running...");

    // Prevent main from exiting
    loop {
        // Here you can add logic to interact with the server or handle other tasks
        // For demonstration, we will just sleep for a while
        tokio::time::sleep(tokio::time::Duration::from_secs(5)).await;
    }
}
