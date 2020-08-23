use anyhow::anyhow;
use btleplug::api::{BDAddr, Central, CentralEvent, Peripheral};
use btleplug::bluez::{adapter::ConnectedAdapter, manager::Manager};
use mijia::connect_sensor;
use std::{collections::HashSet, error::Error};

fn get_central(manager: &Manager) -> ConnectedAdapter {
    let adapters = manager.adapters().unwrap();
    let adapter = adapters.into_iter().nth(0).unwrap();
    adapter.connect().unwrap()
}

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
                    connect_sensor(&device)?;
                    mijia::start_notify_sensor(&device)?;
                }
            }
        }
        e => {
            println!("Other event {:?}", e);
        }
    }
    Ok(())
}
