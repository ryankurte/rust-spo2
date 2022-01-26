
use log::{info, error};

use structopt::StructOpt;
use simplelog::{TermLogger, LevelFilter, ConfigBuilder, TerminalMode, ColorChoice};

use spo2::{Sensor, Options};


#[derive(Clone, PartialEq, Debug, StructOpt)]
pub struct Config {

    #[structopt(flatten)]
    pub options: Options,

    /// Application log level
    #[structopt(long, default_value = "info")]
    pub log_level: LevelFilter,
}



#[tokio::main]
async fn main() {
    // Parse command line arguments
    let cfg = Config::from_args();

    // Setup application logging
    let log_cfg = ConfigBuilder::new().build();
    let _logger = TermLogger::init(cfg.log_level, log_cfg, TerminalMode::Mixed, ColorChoice::Auto);

    // Connect to sensor
    let s = match Sensor::connect(cfg.options).await {
        Ok(s) => s,
        Err(e) => {
            error!("Failed to connect to sensor: {:?}", e);
            return;
        }
    };

    // TODO: whatever

}

