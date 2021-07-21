use drk::cli::{DrkCli, DrkConfig};
use drk::util::join_config_path;
use drk::Result;
use log::*;

use rand::Rng;
extern crate serde_json;
use serde_json::{Map, Value};

use std::path::{Path, PathBuf};

struct Drk {
    url: String,
    payload: Map<String, Value>,
}

impl Drk {
    pub fn new(url: String) -> Self {
        let mut payload = Map::new();

        payload.insert("jsonrpc".into(), Value::String("2.0".into()));

        let id = Self::random_id();

        payload.insert("id".into(), Value::String(id.to_string()));

        Self { payload, url }
    }

    pub fn random_id() -> u32 {
        let mut rng = rand::thread_rng();
        rng.gen()
    }

    pub async fn say_hello(&mut self) -> Result<()> {
        self.payload
            .insert("method".into(), Value::String("say_hello".into()));
        self.request().await
    }

    pub async fn create_cashier_wallet(&mut self) -> Result<()> {
        self.payload.insert(
            "method".into(),
            Value::String("create_cashier_wallet".into()),
        );
        self.request().await
    }

    pub async fn create_wallet(&mut self) -> Result<()> {
        self.payload
            .insert("method".into(), Value::String("create_wallet".into()));
        self.request().await
    }

    pub async fn key_gen(&mut self) -> Result<()> {
        self.payload
            .insert("method".into(), Value::String("key_gen".into()));
        self.request().await
    }

    pub async fn get_info(&mut self) -> Result<()> {
        self.payload
            .insert("method".into(), Value::String("get_info".into()));
        self.request().await
    }

    pub async fn stop(&mut self) -> Result<()> {
        self.payload
            .insert("method".into(), Value::String("stop".into()));
        self.request().await
    }

    pub async fn deposit(&mut self) -> Result<()> {
        self.payload
            .insert(String::from("method"), Value::String("deposit".into()));
        self.request().await
    }

    pub async fn transfer(&mut self, address: String, amount: String) -> Result<()> {
        let mut params = Map::new();
        params.insert("amount".into(), Value::String(amount));
        params.insert("address".into(), Value::String(address));

        self.payload
            .insert(String::from("method"), Value::String("transfer".into()));

        self.payload
            .insert(String::from("params"), Value::Object(params));

        self.request().await
    }

    pub async fn withdraw(&mut self, address: String, amount: String) -> Result<()> {
        let mut params = Map::new();
        params.insert("amount".into(), Value::String(amount));
        params.insert("address".into(), Value::String(address));

        self.payload
            .insert(String::from("method"), Value::String("withdraw".into()));
        self.payload
            .insert(String::from("params"), Value::Object(params));

        self.request().await
    }

    async fn request(&self) -> Result<()> {
        let payload = surf::Body::from_json(&self.payload)?;
        let payload = payload.into_string().await?;

        let mut res = surf::post(&self.url).body(payload).await?;

        if res.status() == 200 {
            let response = res.take_body();
            let response = response.into_string().await?;
            info!("Response Result: {:?}", response);
        }
        Ok(())
    }
}

async fn start(config: &DrkConfig, options: DrkCli) -> Result<()> {
    let url = config.rpc_url.clone();
    let mut client = Drk::new(url);

    if options.cashier {
        client.create_cashier_wallet().await?;
    }

    if options.wallet {
        client.create_wallet().await?;
    }

    if options.key {
        client.key_gen().await?;
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
    use simplelog::*;

    let options = DrkCli::load()?;

    let path = join_config_path(&PathBuf::from("drk.toml")).unwrap();

    let config: DrkConfig = if Path::new(&path).exists() {
        DrkConfig::load(path)?
    } else {
        DrkConfig::load_default(path)?
    };

    let config_ptr = &config;

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

    futures::executor::block_on(start(config_ptr, options))?;

    Ok(())
}
