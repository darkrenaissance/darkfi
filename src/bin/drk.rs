use drk::cli::{ClientCliConfig, DrkCli, DrkCliConfig};
use drk::Result;

use log::*;

use async_std::sync::Arc;
use std::collections::HashMap;
use std::path::PathBuf;

type Payload = HashMap<String, String>;

struct Drk {
    url: String,
    payload: Payload,
}

impl Drk {
    pub fn new(url: String) -> Self {
        let mut payload = HashMap::new();
        payload.insert(String::from("jsonrpc"), String::from("2.0"));
        payload.insert(String::from("id"), String::from("0"));
        Self { payload, url }
    }

    pub async fn say_hello(&mut self) -> Result<()> {
        self.payload
            .insert(String::from("method"), String::from("say_hello"));
        self.request().await
    }

    pub async fn create_cashier_wallet(&mut self) -> Result<()> {
        self.payload.insert(
            String::from("method"),
            String::from("create_cashier_wallet"),
        );
        self.request().await
    }

    pub async fn create_wallet(&mut self) -> Result<()> {
        self.payload
            .insert(String::from("method"), String::from("create_wallet"));
        self.request().await
    }

    pub async fn key_gen(&mut self) -> Result<()> {
        self.payload
            .insert(String::from("method"), String::from("key_gen"));
        self.request().await
    }

    pub async fn get_info(&mut self) -> Result<()> {
        self.payload
            .insert(String::from("method"), String::from("get_info"));
        self.request().await
    }

    pub async fn stop(&mut self) -> Result<()> {
        self.payload
            .insert(String::from("method"), String::from("stop"));
        self.request().await
    }

    async fn request(&self) -> Result<()> {
        let mut res = surf::post(&self.url)
            .body(http_types::Body::from_json(&self.payload)?)
            .await?;

        if res.status() == 200 {
            let response = res.take_body();
            let response = response.into_string().await?;
            info!("Response Result: {:?}", response);
        }
        Ok(())
    }
}

async fn start(config: Arc<DrkCliConfig>, options: Arc<DrkCli>) -> Result<()> {
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

    if options.stop {
        client.stop().await?;
    }

    Ok(())
}

fn main() -> Result<()> {
    use simplelog::*;

    let mut config = DrkCliConfig::load(PathBuf::from("drk_config"))?;
    let options = Arc::new(DrkCli::load(&mut config)?);

    if options.change_config {
        config.save(PathBuf::from("drk_config"))?;
        return Ok(());
    }

    let config = Arc::new(config);

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

    futures::executor::block_on(start(config, options))?;

    Ok(())
}
