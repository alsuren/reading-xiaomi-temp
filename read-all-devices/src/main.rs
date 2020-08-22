use anyhow::anyhow;
use btleplug::api::Peripheral;
use btleplug::api::{Central, CentralEvent};
use btleplug::bluez::{adapter::ConnectedAdapter, manager::Manager};
use std::error::Error;

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
    while let Ok(event) = event_receiver.recv() {
        match on_event(&central, event) {
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
            let props = central
                .peripheral(bd_addr)
                .ok_or_else(|| anyhow!("missing peripheral {}", bd_addr))?
                .properties();
            println!("DeviceUpdated: {:?}, {:?}", bd_addr, props);
        }
        e => {
            println!("Other event {:?}", e);
        }
    }
    Ok(())
}
