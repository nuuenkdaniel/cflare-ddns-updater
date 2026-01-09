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

#[derive(Deserialize, Debug)]
struct ZoneInfo {
    result: Vec<ZoneInfoResult>,
    success: bool,
}

#[derive(Deserialize, Debug)]
struct ZoneInfoResult {
    id: String,
    name: String,
    // status: String,
}

async fn get_zone_id(record_name: &str, bearer: &str) -> Result<String, Box<dyn Error>> {
    let client = Client::new();
    let url: String = format!("https://api.cloudflare.com/client/v4/zones/");
    let zone_info = client
        .get(url)
        .header("Authorization", format!("Bearer {}", bearer))
        .send()
        .await?
        .json::<ZoneInfo>()
        .await?;
    if zone_info.success == true {
        let mut split_domain: Vec<&str> = record_name.split('.').collect();
        let tld = split_domain.pop().unwrap_or("");
        let next_ld = split_domain.pop().unwrap_or("");
        let base_domain = format!("{}.{}", next_ld, tld);
        println!("Getting zone_id for {}...", base_domain);
        for info in zone_info.result {
            if info.name == base_domain {
                println!("zone_id for {}: {}", base_domain, info.id);
                return Ok(info.id);
            }
        }
    }
    Ok("".to_string())
}

#[derive(Deserialize, Debug)]
struct RecordInfo {
    result: Vec<RecordResult>,
    success: bool,
}

#[derive(Deserialize, Debug)]
struct RecordResult {
    name: String,
    content: String,
}

async fn get_cflare_ip(record_name: &str, bearer: &str) -> Result<String, Box<dyn Error>> {
    println!("\nGetting current ip for {} from cloudflare...", record_name);
    let zone_id = get_zone_id(record_name, bearer).await?;
    let client = Client::new();
    let url: String = format!("https://api.cloudflare.com/client/v4/zones/{}/dns_records", zone_id);
    let record_info = client
        .get(url)
        .header("Authorization", format!("Bearer {}", bearer))
        .send()
        .await?
        .json::<RecordInfo>()
        .await?;
    if record_info.success == true {
        for record in record_info.result {
            if record.name == record_name {
                println!("IP is set to {} on cloudflare", record.content);
                return Ok(record.content);
            }
        }
    }
    Ok("".to_string())
}

// async fn update_cflare_ip(record_name: &str, new_ip: &str) {
//     todo!("Implement updating cflare ip with new_ip");
// }

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    get_pub_ip().await?;
    get_cflare_ip("", "").await?;
    Ok(())
}
