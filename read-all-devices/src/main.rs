use anyhow::anyhow;
use btleplug::api::{BDAddr, Central, CentralEvent};
use btleplug::bluez::{adapter::ConnectedAdapter, manager::Manager};
use mijia::{connect_sensor, hashmap_from_file, FailureCompat};
use std::{
    collections::{HashMap, VecDeque},
    time::{Duration, Instant},
};

fn main() -> anyhow::Result<()> {
    let sensor_names: HashMap<BDAddr, String> = hashmap_from_file("sensor_names.conf")?;

    let manager = Manager::new().unwrap();
    let adapter = manager.adapters().unwrap().into_iter().nth(0).unwrap();
    manager.down(&adapter).compat()?;
    manager.up(&adapter).compat()?;
    let central = adapter.connect().compat()?;
    let event_receiver = central.event_receiver().unwrap();

    println!("Scanning");
    central.filter_duplicates(false);
    central.start_scan().unwrap();

    println!("waiting");

    let mut sensors_to_connect = VecDeque::new();
    loop {
        let start = Instant::now();
        let next_timeout = start + Duration::from_secs(5);
        while let Ok(event) = event_receiver.recv_timeout(Duration::from_secs(5)) {
            if let Some(bd_addr) = event_to_address(event) {
                if let Some(name) = sensor_names.get(&bd_addr).map(String::as_str) {
                    println!("Enqueueing {:?} {:?}", bd_addr, name);
                    sensors_to_connect.push_back(bd_addr);
                }
            }
            if Instant::now() > next_timeout {
                break;
            }
        }
        println!("Connecting n of {:?}", sensors_to_connect.len());
        if let Some(bd_addr) = sensors_to_connect.pop_front() {
            let name: &str = sensor_names
                .get(&bd_addr)
                .map(String::as_str)
                .unwrap_or_default();
            connect_and_subscribe(&central, bd_addr)
                .map(|()| {
                    println!("connected to: {:?} {:?}", bd_addr, name);
                })
                .unwrap_or_else(|e| {
                    println!("error connecting to {:?} {:?}: {:?}", bd_addr, name, e);
                    sensors_to_connect.push_back(bd_addr);
                })
        };
    }
}

fn event_to_address(event: CentralEvent) -> Option<BDAddr> {
    match event {
        CentralEvent::DeviceDiscovered(bd_addr) => Some(bd_addr),
        _ => None,
    }
}

fn connect_and_subscribe(central: &ConnectedAdapter, bd_addr: BDAddr) -> anyhow::Result<()> {
    let device = central
        .peripheral(bd_addr)
        .ok_or_else(|| anyhow!("missing peripheral {}", bd_addr))?;
    connect_sensor(&device)?;
    mijia::start_notify_sensor(&device)?;
    Ok(())
}
