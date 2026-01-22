use reqwest::Client;
use serde::Deserialize;
use serde_json::json;
use dotenv::dotenv;
use anyhow::{Context, Result, bail};
use tracing::{info, error, warn, debug, instrument};
use std::time::{Duration, Instant};
use tokio::time::sleep;

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

#[instrument(skip(client))]
async fn get_pub_ip(client: &Client) -> Result<String> {
    debug!("Getting public IP...");
    let public_ip = client
        .get("https://api.ipify.org?format=json")
        .send()
        .await
        .context("Failed to connect to ipify")?
        .json::<IpResp>()
        .await
        .context("Failed to optain ip from json")?
        .ip;
    debug!("Public IP: {}", public_ip);
    Ok(public_ip)
}

impl Cloudflare {
    #[instrument(skip(self))]
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
        let tld = split_domain[split_domain.len()-1]; let domain = split_domain[split_domain.len()-2];
        let base_domain = format!("{}.{}", domain, tld);
        if zone_info.success == true {
            debug!("Getting zone_id for {}...", base_domain);
            for info in zone_info.result {
                if info.name == base_domain {
                    debug!("zone_id for {}: {}", base_domain, info.id);
                    return Ok(info.id);
                }
            }
        }
        bail!("Zone ID could not be found for: {}", base_domain)
    }

    #[instrument(skip(self))]
    async fn get_cflare_ip(
        &self,
        zone_id: &str,
        record_name: &str,
    ) -> Result<(String, String)> {
        debug!("Getting current ip for {} from cloudflare...", record_name);
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
                    debug!("Record ID: {}", record.id);
                    debug!("IP is set to {} on cloudflare", record.content);
                    return Ok((record.id, record.content));
                }
            }
        }
        bail!("Failed to get the current ip for {}", record_name)
    }

    #[instrument(skip(self))]
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

#[tokio::main]
async fn main() -> Result<()> {
    // Load env vars
    dotenv().ok();
    let api_key = env::var("CFLARE_API_KEY").expect("CFLARE_API_KEY not found");
    let record_name = env::var("DOMAIN_RECORD").expect("DOMAIN_RECORD not found");
    let cflare_sync_interval_raw = env::var("CFLARE_SYNC_INTERVAL").unwrap_or_default();
    let ip_check_interval_raw = env::var("IP_CHECK_INTERVAL").unwrap_or_default();

    let cflare_sync_interval: u64 = cflare_sync_interval_raw.parse().unwrap_or(43200);
    let ip_check_interval: u64 = ip_check_interval_raw.parse().unwrap_or(300);

    tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::try_from_default_env()
            .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info")))
        .init();

    let client = Client::new();

    let cflare_api = Cloudflare {
        api_key: api_key,
        client: client.clone(),
    };

    let mut fail_count = 0;
    let mut last_known_ip: String = "".to_string();
    let mut zone_id: String = "".to_string();
    let mut record_id: String = "".to_string();
    let mut last_sync: Instant = Instant::now() - Duration::from_secs(999999);
    let sync_interval: Duration = Duration::from_secs(cflare_sync_interval);
    info!("Started");
    loop {
        if fail_count >= 10 { error!("Too many errors, Please check your internet connection/configuration") };

        // Get public ip
        info!("Checking ip changes");
        let public_ip: String = match get_pub_ip(&client).await {
            Ok(ip) => ip,
            Err(e) => {
                warn!("Failed to get public ip: {} | Retrying in 30 Seconds...", e);
                fail_count += 1;
                sleep(Duration::from_secs(30)).await;
                continue;
            }
        };

        if last_sync.elapsed() >= sync_interval {
            // Get record info
            info!("Syncing last known ip with cloudflare");
            zone_id = match cflare_api.get_zone_id(&record_name).await {
                Ok(id) => id,
                Err(e) => {
                    warn!("Failed to get zone id for {}: {} | Retrying in 30 Seconds...", record_name, e);
                    fail_count += 1;
                    sleep(Duration::from_secs(30)).await;
                    continue;
                }
            };
            let cflare_record: (String, String) = match cflare_api.get_cflare_ip(&zone_id, &record_name).await {
                Ok(record) => {
                    last_sync = Instant::now();
                    record
                },
                Err(e) => {
                    warn!("Failed to get record for {}: {} | Retrying in 60 Seconds...", record_name, e);
                    fail_count += 1;
                    sleep(Duration::from_secs(30)).await;
                    continue;
                }
            };
            record_id = cflare_record.0;
            last_known_ip = cflare_record.1;
        }

        // Check for change and update
        if public_ip != last_known_ip {
            info!("IP change detected, updating ip for {}...", record_name);
            cflare_api.update_cflare_ip(&zone_id, &record_id, &public_ip).await?;
            info!("Updated ip to {}", public_ip);
        }
        else { info!("No change") };
        fail_count = 0;
        sleep(Duration::from_secs(ip_check_interval)).await;
    }
}
