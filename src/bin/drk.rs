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

    async fn request(&self) -> Result<()> {
        let mut res = surf::post(&self.url)
            .body(http_types::Body::from_json(&self.payload)?)
            .await?;

        if res.status() == 200 {
            let response = res.body_json::<Payload>().await?;
            info!("Response Result: {:?}", response);
        }
        Ok(())
    }
}

async fn start(config: Arc<DrkCliConfig>) -> Result<()> {
    let url = config.rpc_url.clone();

    let mut client = Drk::new(url);
    client.say_hello().await?;

    Ok(())
}

fn main() -> Result<()> {
    use simplelog::*;

    let mut config = DrkCliConfig::load(PathBuf::from("drk_config_file"))?;
    let options = Arc::new(DrkCli::load(&mut config)?);

    if options.change_config {
        config.save(PathBuf::from("drk_config_file"))?;
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

    futures::executor::block_on(start(config))?;

    Ok(())
}
