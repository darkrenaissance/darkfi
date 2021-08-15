use std::path::{Path, PathBuf};

use serde_json::json;

use drk::cli::{Config, DrkCli, DrkConfig};
use drk::rpc::jsonrpc;
use drk::rpc::jsonrpc::JsonResult;
use drk::util::join_config_path;
use drk::{Error, Result};

use log::info;

struct Drk {
    url: String,
}

impl Drk {
    pub fn new(url: String) -> Self {
        Self { url }
    }

    async fn request(&self, method_name: &str, r: jsonrpc::JsonRequest) -> Result<()> {
        // TODO: Return actual JSON result
        let data = surf::Body::from_json(&r)?;
        info!("--> {:?}", r);
        let mut req = surf::post(&self.url).body(data).await?;

        let resp = req.take_body();
        let json = resp.into_string().await?;

        let v: JsonResult = serde_json::from_str(&json)?;
        match v {
            JsonResult::Resp(r) => {
                info!("<-- {:?}", r);
                println!("{}: {}", method_name, r.result);
                return Ok(());
            }

            JsonResult::Err(e) => {
                info!("<-- {:?}", e);
                return Err(Error::JsonRpcError(e.error.message.to_string()));
            }
        };
    }

    pub async fn say_hello(&self) -> Result<()> {
        let r = jsonrpc::request(json!("say_hello"), json!([]));
        Ok(self.request("say hello", r).await?)
    }

    pub async fn create_wallet(&self) -> Result<()> {
        let r = jsonrpc::request(json!("create_wallet"), json!([]));
        Ok(self.request("create wallet", r).await?)
    }

    pub async fn key_gen(&self) -> Result<()> {
        let r = jsonrpc::request(json!("key_gen"), json!([]));
        Ok(self.request("key gen", r).await?)
    }

    pub async fn get_key(&self) -> Result<()> {
        let r = jsonrpc::request(json!("get_key"), json!([]));
        Ok(self.request("get key", r).await?)
    }

    pub async fn get_info(&self) -> Result<()> {
        let r = jsonrpc::request(json!("get_info"), json!([]));
        Ok(self.request("get info", r).await?)
    }

    pub async fn stop(&self) -> Result<()> {
        let r = jsonrpc::request(json!("stop"), json!([]));
        Ok(self.request("stop", r).await?)
    }

    pub async fn deposit(&self) -> Result<()> {
        let r = jsonrpc::request(json!("deposit"), json!([]));
        Ok(self.request("deposit BTC to this address:", r).await?)
    }

    pub async fn transfer(&self, address: String, amount: f64) -> Result<()> {
        let r = jsonrpc::request(json!("transfer"), json!([address, amount]));
        Ok(self.request("transfer", r).await?)
    }

    pub async fn withdraw(&self, address: String, amount: f64) -> Result<()> {
        let r = jsonrpc::request(json!("withdraw"), json!([address, amount]));
        Ok(self.request("withdraw", r).await?)
    }
}

async fn start(config: &DrkConfig, options: DrkCli) -> Result<()> {
    let url = config.rpc_url.clone();
    let client = Drk::new(url);

    if options.wallet {
        client.create_wallet().await?;
    }

    if options.key {
        client.key_gen().await?;
    }

    if options.get_key {
        client.get_key().await?;
    }

    if options.info {
        client.get_info().await?;
    }

    if options.hello {
        client.say_hello().await?;
    }

    if let Some(transfer) = options.transfer {
        client.transfer(transfer.pub_key, transfer.amount).await?;
    }

    if let Some(_deposit) = options.deposit {
        client.deposit().await?;
    }

    if let Some(withdraw) = options.withdraw {
        client.withdraw(withdraw.pub_key, withdraw.amount).await?;
    }

    if options.stop {
        client.stop().await?;
    }

    Ok(())
}

fn main() -> Result<()> {
    let options = DrkCli::load()?;

    let config_path: PathBuf;

    match options.config.as_ref() {
        Some(path) => {
            config_path = path.to_owned();
        }
        None => {
            config_path = join_config_path(&PathBuf::from("drk.toml"))?;
        }
    }

    let config: DrkConfig = if Path::new(&config_path).exists() {
        Config::<DrkConfig>::load(config_path)?
    } else {
        Config::<DrkConfig>::load_default(config_path)?
    };

    {
        use simplelog::*;
        let logger_config = ConfigBuilder::new().set_time_format_str("%T%.6f").build();

        let debug_level = if options.verbose {
            LevelFilter::Info
        } else {
            LevelFilter::Off
        };

        let log_path = config.log_path.clone();
        CombinedLogger::init(vec![
            TermLogger::new(debug_level, logger_config, TerminalMode::Mixed).unwrap(),
            WriteLogger::new(
                LevelFilter::Debug,
                Config::default(),
                std::fs::File::create(log_path).unwrap(),
            ),
        ])
        .unwrap();
    }

    futures::executor::block_on(start(&config, options))
}
