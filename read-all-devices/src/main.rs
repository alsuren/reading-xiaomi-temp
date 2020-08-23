use anyhow::{anyhow, Context};
use btleplug::api::{BDAddr, Central, CentralEvent, Peripheral, UUID};
use btleplug::bluez::{adapter::ConnectedAdapter, manager::Manager};
use failure::Fail;
use std::str::FromStr;
use std::{collections::HashSet, error::Error};

fn get_central(manager: &Manager) -> ConnectedAdapter {
    let adapters = manager.adapters().unwrap();
    let adapter = adapters.into_iter().nth(0).unwrap();
    adapter.connect().unwrap()
}

const READINGS_ID: &str = "EB:E0:CC:C1:7A:0A:4B:0C:8A:1A:6F:F2:99:7D:A3:A6";

fn main() {
    // let sensor_names = hashmap_from_file(SENSOR_NAMES_FILENAME).unwrap();

    let manager = Manager::new().unwrap();
    let central = get_central(&manager);
    let event_receiver = central.event_receiver().unwrap();

    // FIXME: turn the bluetooth adapter on?
    println!("Scanning");
    central.start_scan().unwrap();

    println!("waiting");
    let mut seen = Default::default();
    while let Ok(event) = event_receiver.recv() {
        match on_event(&central, event, &mut seen) {
            Ok(()) => {}
            Err(err) => {
                dbg!(err);
            }
        }
    }
}

fn on_event(
    central: &ConnectedAdapter,
    event: CentralEvent,
    seen: &mut HashSet<BDAddr>,
) -> Result<(), Box<dyn Error + Send + Sync>> {
    match event {
        CentralEvent::DeviceDiscovered(bd_addr) => {
            println!("DeviceDiscovered: {:?}", bd_addr);
        }
        CentralEvent::DeviceConnected(bd_addr) => {
            println!("DeviceConnected: {:?}", bd_addr);
        }
        CentralEvent::DeviceDisconnected(bd_addr) => {
            println!("DeviceDisconnected: {:?}", bd_addr);
        }
        CentralEvent::DeviceUpdated(bd_addr) => {
            if !seen.contains(&bd_addr) {
                let device = central
                    .peripheral(bd_addr)
                    .ok_or_else(|| anyhow!("missing peripheral {}", bd_addr))?;
                let props = device.properties();

                println!(
                    "DeviceUpdated: {:?}, {:?}, {:?}",
                    bd_addr,
                    device.is_connected(),
                    props,
                );
                seen.insert(bd_addr);

                if props.local_name == Some("LYWSD03MMC".into()) {
                    device
                        .connect()
                        .map_err(|err| err.compat())
                        .with_context(|| format!("connecting to {:?}", bd_addr))?;
                    device
                        .discover_characteristics()
                        .map_err(|err| err.compat())
                        .context("discovering characteristics")?
                        .iter()
                        .find(|c| c.uuid == UUID::from_str(READINGS_ID).unwrap())
                        .map(|c| {
                            device
                                .subscribe(c)
                                .map_err(|err| err.compat())
                                .context("subscribing to readings")
                        })
                        .transpose()?;

                    device.on_notification(Box::new(move |val| {
                        println!("on_notification: {:?} {:?}", bd_addr, val)
                    }));
                }
            }
        }
        e => {
            println!("Other event {:?}", e);
        }
    }
    Ok(())
}
