use reqwest::Client;
use serde::Deserialize;
use serde_json::json;
use dotenv::dotenv;

use std::error::Error;
use std::env;

#[derive(Deserialize)]
struct IpResp {
    ip: String,
}

async fn get_pub_ip() -> Result<String, Box<dyn Error>> {
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
    return Ok(public_ip);
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
    id: String,
    name: String,
    content: String,
}

async fn get_cflare_ip(zone_id: &str, record_name: &str, bearer: &str) -> Result<(String, String), Box<dyn Error>> {
    println!("\nGetting current ip for {} from cloudflare...", record_name);
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
                return Ok((record.id, record.content));
            }
        }
    }
    Ok(("".to_string(), "".to_string()))
}

async fn update_cflare_ip(zone_id: &str, record_id: &str, new_ip: &str, bearer: &str) -> Result<(), Box<dyn Error>> {
    let client = Client::new();
    let url: String = format!("https://api.cloudflare.com/client/v4/zones/{}/dns_records/{}", zone_id, record_id);
    client.patch(url)
        .header("Content-Type", "application/json")
        .header("Authorization", format!("Bearer {}", bearer))
        .json(&json!({
            "content": new_ip
        }))
    .send()
        .await?;
    Ok(())
}

// TODO: Reuse client instead of opening new client everytime
#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    // Load env vars
    dotenv().ok();
    let api_key: String = env::var("CFLARE_API_KEY").expect("CFLARE_API_KEY not found");
    let record_name: String = env::var("DOMAIN_RECORD").expect("DOMAIN_RECORD not found");

    // Get record info
    let public_ip: String = get_pub_ip().await?;
    let zone_id = get_zone_id(&record_name, &api_key).await?;
    let cflare_record: (String, String) = get_cflare_ip(&zone_id, &record_name, &api_key).await?;
    let cflare_id: String = cflare_record.0;
    let cflare_ip: String = cflare_record.1;
    println!("Record ID: {}", cflare_id);

    // Check for change and update
    if public_ip != cflare_ip {
        println!("Updating ip for {}...", record_name);
        update_cflare_ip(&zone_id, &cflare_id, &public_ip, &api_key).await?;
        println!("Updated ip to {}", public_ip);
    }
    else {
        println!("No change");
    }

    Ok(())
}
