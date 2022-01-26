
use std::time::{Duration, Instant};

use futures::stream::StreamExt;
use log::{debug, info, error};

use btleplug::platform::{Peripheral, Manager};
use btleplug::api::{ScanFilter, Manager as _, Central as _, Peripheral as _, CentralEvent};

use structopt::StructOpt;


#[derive(Debug)]
pub struct Sensor {
    p: Peripheral,
}

#[derive(Debug, PartialEq, Clone, StructOpt)]
pub struct Options {
    /// BLE adaptor to use for discovery and connection
    #[structopt(long, default_value="0")]
    pub adaptor: usize,

    /// Device local name
    #[structopt(long, default_value="J1")]
    pub local_name: String,

    /// Timeout for search operation
    #[structopt(long, default_value="10s")]
    pub search_timeout: humantime::Duration,
}

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("Bluetooth: {0}")]
    Ble(btleplug::Error),

    #[error("No matching adaptor for index {0}")]
    NoMatchingAdaptor(usize),

    #[error("No device found")]
    NoDeviceFound,
}

impl From<btleplug::Error> for Error {
    fn from(e: btleplug::Error) -> Self {
        Error::Ble(e)
    }
}

impl Sensor {
    pub async fn connect(opts: Options) -> Result<Self, Error> {

        // Connect to BLE manager
        let manager = Manager::new().await?;

        // Fetch adapter for central role
        let adapters = manager.adapters().await?;
        let central = match adapters.into_iter().nth(opts.adaptor) {
            Some(c) => c,
            None => {
                return Err(Error::NoMatchingAdaptor(opts.adaptor));
            }
        };

        // Setup event channel
        let mut events = central.events().await?;

        // Start scanning
        debug!("Starting scan for BLE devices");
        central.start_scan(ScanFilter::default()).await?;

        let mut device = None;

        let now = Instant::now();
        while now.elapsed() < *opts.search_timeout {

            // Wait for incoming events
            let evt = match tokio::time::timeout(Duration::from_millis(500), events.next() ).await {
                Ok(Some(e)) => e,
                // TODO: separate BLE errors from timeout
                _ => continue,
            };

            match evt {
                CentralEvent::DeviceDiscovered(id) => {
                    // Fetch peripheral information
                    let periph = central.peripheral(&id).await?;
                    debug!("Discovered peripheral {:?}: {:?}", id, periph);

                    // Read out properties
                    let props = match periph.properties().await {
                        Ok(Some(p)) => p,
                        Ok(None) => {
                            error!("Failed to fetch properties for peripheral {:?}", id);
                            continue;
                        },
                        Err(e) => {
                            error!("Failed to fetch properties for peripheral {:?}: {:?}", periph, e);
                            continue;
                        },
                    };
                    debug!("Properties: {:?}", props);

                    // TODO: match filters / return device
                    match &props.local_name {
                        Some(l) if l.starts_with(&opts.local_name) => {
                            info!("Matching device!: {:?}", props);
                            device = Some(periph);
                            break;
                        },
                        _ => continue,
                    }


                },
                _ => (),
            };
        }

        // Stop scanning
        central.stop_scan().await?;

        // Check we actually found something
        let device = match device {
            Some(d) => d,
            None => return Err(Error::NoDeviceFound)
        };

        // TODO: start listener task, subscribe to notifications? though this could also be part of Sensor API

        // Return device
        Ok(Self{
            p: device,
        })
    }
}
