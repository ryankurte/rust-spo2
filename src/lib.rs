
use std::time::{Duration, Instant};

use futures::stream::StreamExt;
use log::{trace, debug, info, error};

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
    #[structopt(long, default_value="20s")]
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
    
    #[error("Failed to connect to device")]
    ConnectFailed,

    #[error("Failed to discover services for device")]
    NoServicesFound,
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
        let mut pid = None;

        let now = Instant::now();
        while now.elapsed() < *opts.search_timeout {

            // Wait for incoming events
            let evt = match tokio::time::timeout(Duration::from_millis(500), events.next() ).await {
                Ok(Some(e)) => e,
                // TODO: separate BLE errors from timeout
                _ => continue,
            };

            match (&evt, &pid) {
                (CentralEvent::DeviceDiscovered(id), None) => {
                    // Fetch peripheral information
                    let periph = central.peripheral(&id).await?;
                    trace!("Discovered peripheral {:?}: {:?}", id, periph);

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
                    trace!("Properties: {:?}", props);

                    // TODO: match filters / return device
                    match &props.local_name {
                        Some(l) if l.starts_with(&opts.local_name) => {
                            info!("Matching device!: {:?}", props);

                            if !periph.is_connected().await? {
                                periph.connect().await?;
                            } else {
                                periph.discover_services().await?;
                            }

                            device = Some((periph, props));
                            pid = Some(id.clone());
                        },
                        _ => (),
                    }
                },
                (CentralEvent::DeviceConnected(id), Some(pid)) if id == pid => {
                    debug!("Connected event for {:?}", id);

                    device.as_ref().unwrap().0.discover_services().await?;
                },
                (CentralEvent::DeviceDisconnected(id), Some(pid)) if id == pid => {
                    debug!("Disconnected event for {:?}", id)  
                },
                (CentralEvent::ManufacturerDataAdvertisement{id, manufacturer_data}, Some(pid)) if id == pid => {
                    debug!("Manufacturer info for {:?}: {:?}", id, manufacturer_data);
                }
                (CentralEvent::ServicesAdvertisement{id, services}, Some(pid)) if id == pid => {
                    debug!("Service advertisement for {:?}: {:?}", id, services);
                },
                (CentralEvent::ServiceDataAdvertisement{id, service_data}, Some(pid)) if id == pid => {
                    debug!("Service data advertisement for {:?}: {:?}", id, service_data);
                },
                (CentralEvent::DeviceUpdated(id), Some(pid)) if id == pid => {
                    debug!("Updated event for {:?}", id);
                },
                _ => {
                    trace!("Unhandled event: {:?}", evt)
                }
            };
        }

        drop(events);

        // Stop scanning
        //central.stop_scan().await?;

        #[cfg(nope)]
        let (device, props) = match &pid {
            Some(pid) => {
                let periph = central.peripheral(pid).await?;
                let props = periph.properties().await?.unwrap();
                (periph, props)
            },
            None => return Err(Error::NoDeviceFound),
        };

        // Check we found a useful device
        let (device, props) = match device {
            Some(d) => d,
            None => return Err(Error::NoDeviceFound)
        };

        // Ensure we're connected
        if !device.is_connected().await? {
            debug!("Connecting to device");
            device.connect().await?;

            if !device.is_connected().await? {
                error!("Failed to connect to device");
                return Err(Error::ConnectFailed)
            }
        }
        debug!("Connected to peripheral: {:?}", props.local_name);


        // Discover services then characteristics
        debug!("Discovering services");

        device.discover_services().await?;
        if device.services().is_empty() {
            return Err(Error::NoServicesFound)
        }

        for service in device.services() {
            debug!("Service: {}, primary: {}", service.uuid, service.primary);
            
            for char in service.characteristics {
                debug!("  - {:?}", char);
            }
        }

        // TODO: start listener task, subscribe to notifications? though this could also be part of Sensor API

        // Return device
        Ok(Self{
            p: device,
        })
    }
}
