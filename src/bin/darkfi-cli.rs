use drk::cli::{ClientCliConfig, DarkfiCli, DarkfiCliConfig};
use drk::Result;

use async_executor::Executor;
use easy_parallel::Parallel;

use async_std::sync::Arc;
use std::path::PathBuf;

async fn start(_executor: Arc<Executor<'_>>, _config: Arc<DarkfiCliConfig>) -> Result<()> {
    Ok(())
}

fn main() -> Result<()> {
    use simplelog::*;

    let mut config = DarkfiCliConfig::load(PathBuf::from("darkfi_config_file"))?;
    let options = Arc::new(DarkfiCli::load(&mut config)?);

    if options.change_config {
        config.save(PathBuf::from("darkfi_config_file"))?;
        return Ok(());
    }

    let config = Arc::new(config);

    let ex = Arc::new(Executor::new());
    let (signal, shutdown) = async_channel::unbounded::<()>();

    let logger_config = ConfigBuilder::new().set_time_format_str("%T%.6f").build();

    let debug_level = if options.verbose {
        LevelFilter::Debug
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

    let ex2 = ex.clone();

    let (_, result) = Parallel::new()
        // Run four executor threads.
        .each(0..3, |_| smol::future::block_on(ex.run(shutdown.recv())))
        // Run the main future on the current thread.
        .finish(|| {
            smol::future::block_on(async move {
                start(ex2, config).await?;
                drop(signal);
                Ok::<(), drk::Error>(())
            })
        });

    result
}
