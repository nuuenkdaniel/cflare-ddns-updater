use reqwest::Client;
use serde::Deserialize;
use std::error::Error;

#[derive(Deserialize)]
struct IpResp {
    ip: String,
}

async fn get_pub_ip() -> Result<(), Box<dyn Error>> {
    // Get public ip
    println!("Make sure you're not using a VPN, getting public IP...");
    let client = Client::new(); 
    let public_ip = client
        .get("https://api.ipify.org?format=json")
        .send()
        .await?
        .json::<IpResp>()
        .await?
        .ip;
    println!("Public IP: {}", public_ip);
    Ok(())
}

async fn get_cflare_ip(record_name: &str) {
    todo!("Implement checking ip in cloudflare using cloudflare api");
}

async fn update_cflare_ip(record_name: &str, new_ip: &str) {
    todo!("Implement updating cflare ip with new_ip");
}

async fn get_dns(cname: &str) {
    todo!("Implement checking dns records with dns resolver");
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    get_pub_ip().await?;
    Ok(())
}
