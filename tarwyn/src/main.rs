use std::{sync::Arc, thread::sleep};
use tokio::{task, time::Duration};
use tarwyn::{tarwyn_client::TarwynClient, tarwyn_server::TarwynServer};

//simple usage of using tarwyn server and tarwyn client
#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    task::spawn_blocking(|| {
        let tarwyn_server = TarwynServer::new();
        tarwyn_server.start();
        tarwyn_server.stop();
        tarwyn_server.start();
    });

    let tarwyn_client = Arc::new(TarwynClient::new());
    tarwyn_client.start();

    sleep(Duration::from_secs(1));

    let unsubscribe = tarwyn_client.subscribe("hello", |data| {
        println!("this should show show up once {:?}", data);
    });

    let _ = tarwyn_client.subscribe("hello",|data| {
        println!("this should always show up {:?}", data);
    });

    unsubscribe(); //this makes subscribing not happen again

    // Prevent main from exiting
    loop {
        tokio::time::sleep(Duration::from_secs(1)).await;
        tarwyn_client.send_string("hello", "heheha");
        tokio::time::sleep(Duration::from_secs(1)).await;
        let value = tarwyn_client.get("hello");
        println!("got value {:?}", value);
    }
}
