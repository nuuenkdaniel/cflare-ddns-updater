use reqwest::Client;
use serde::Deserialize;
use serde_json::json;
use dotenv::dotenv;
use anyhow::{Context, Result, bail};

use std::env;


#[derive(Deserialize)]
struct IpResp {
    ip: String,
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

struct Cloudflare {
    api_key: String,
    client: Client,
}

async fn get_pub_ip(client: &Client) -> Result<String> {
    println!("Make sure you're not using a VPN, getting public IP...");
    let public_ip = client
        .get("https://api.ipify.org?format=json")
        .send()
        .await
        .context("Failed to query public ip")?
        .json::<IpResp>()
        .await
        .context("Failed to optain ip from json")?
        .ip;
    println!("Public IP: {}", public_ip);
    Ok(public_ip)
}

impl Cloudflare {
    async fn get_zone_id(
        &self,
        record_name: &str,
    ) -> Result<String> {
        let url: String = format!("https://api.cloudflare.com/client/v4/zones/");
        let zone_info = self.client
            .get(url)
            .header("Authorization", format!("Bearer {}", self.api_key))
            .send()
            .await
            .context("Failed to connect to cloudflare")?
            .json::<ZoneInfo>()
            .await
            .context("Failed to obtain json for ZoneInfo")?;

        let split_domain: Vec<&str> = record_name.split('.').collect();
        if split_domain.len() < 2 {
            bail!("{} is not a valid domain format", record_name);
        }
        let tld = split_domain[split_domain.len()-1];
        let domain = split_domain[split_domain.len()-2];
        let base_domain = format!("{}.{}", domain, tld);
        if zone_info.success == true {
            println!("Getting zone_id for {}...", base_domain);
            for info in zone_info.result {
                if info.name == base_domain {
                    println!("zone_id for {}: {}", base_domain, info.id);
                    return Ok(info.id);
                }
            }
        }
        bail!("Zone ID could not be found for: {}", base_domain)
    }

    async fn get_cflare_ip(
        &self,
        zone_id: &str,
        record_name: &str,
    ) -> Result<(String, String)> {
        println!("\nGetting current ip for {} from cloudflare...", record_name);
        let url: String = format!("https://api.cloudflare.com/client/v4/zones/{}/dns_records", zone_id);
        let record_info = self.client
            .get(url)
            .header("Authorization", format!("Bearer {}", self.api_key))
            .send()
            .await
            .context("Failed to connect to cloudflare")?
            .json::<RecordInfo>()
            .await
            .context("Failed to obtain json for RecordInfo")?;
        if record_info.success == true {
            for record in record_info.result {
                if record.name == record_name {
                    println!("IP is set to {} on cloudflare", record.content);
                    return Ok((record.id, record.content));
                }
            }
        }
        bail!("Failed to get the current ip for {}", record_name)
    }

    async fn update_cflare_ip(
        &self,
        zone_id: &str,
        record_id: &str,
        new_ip: &str,
    ) -> Result<()> {
        let url: String = format!("https://api.cloudflare.com/client/v4/zones/{}/dns_records/{}", zone_id, record_id);
        let resp = self.client
            .patch(url)
            .header("Content-Type", "application/json")
            .header("Authorization", format!("Bearer {}", self.api_key))
            .json(&json!({
                "content": new_ip
            }))
            .send()
            .await
            .context("Failed to patch IP")?;
        if !resp.status().is_success() {
            bail!("Failed to update IP");
        }
        Ok(())
    }
}

// TODO: Setup logging
#[tokio::main]
async fn main() -> Result<()> {
    // Load env vars
    dotenv().ok();
    let api_key: String = env::var("CFLARE_API_KEY").expect("CFLARE_API_KEY not found");
    let record_name: String = env::var("DOMAIN_RECORD").expect("DOMAIN_RECORD not found");

    let client = Client::new();

    // Get record info
    let cloudflare_calls = Cloudflare {
        api_key: api_key,
        client: client.clone(),
    };
    let public_ip: String = get_pub_ip(&client).await?;
    let zone_id: String = cloudflare_calls.get_zone_id(&record_name).await?;
    let cflare_record: (String, String) = cloudflare_calls.get_cflare_ip(&zone_id, &record_name).await?;
    let cflare_id: String = cflare_record.0;
    let cflare_ip: String = cflare_record.1;
    println!("Record ID: {}", cflare_id);

    // Check for change and update
    if public_ip != cflare_ip {
        println!("Updating ip for {}...", record_name);
        cloudflare_calls.update_cflare_ip(&zone_id, &cflare_id, &public_ip).await?;
        println!("Updated ip to {}", public_ip);
    }
    else {
        println!("No change");
    }
    Ok(())
}
