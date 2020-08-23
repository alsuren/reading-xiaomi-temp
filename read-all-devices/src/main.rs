use anyhow::anyhow;
use btleplug::api::{BDAddr, Central, CentralEvent};
use btleplug::bluez::{adapter::ConnectedAdapter, manager::Manager};
use mijia::{connect_sensor, hashmap_from_file, FailureCompat};
use std::{
    collections::VecDeque,
    time::{Duration, Instant},
};

fn main() -> anyhow::Result<()> {
    let sensor_names = hashmap_from_file("sensor_names.conf")?;

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
            event_to_address(event)
                .and_then(|bd_addr| sensor_names.get_key_value(&bd_addr))
                .map(|(bd_addr, name): (&BDAddr, &String)| {
                    println!("Enqueueing {:?}", name);
                    sensors_to_connect.push_back(*bd_addr);
                });
            if Instant::now() > next_timeout {
                break;
            }
        }
        println!("Connecting n of {:?}", sensors_to_connect.len());
        if let Some(bd_addr) = sensors_to_connect.pop_front() {
            connect_and_subscribe(&central, bd_addr)
                .map(|()| {
                    println!(
                        "connected to: {:?} {:?}",
                        bd_addr,
                        sensor_names.get(&bd_addr)
                    )
                })
                .unwrap_or_else(|e| {
                    println!("error connecting: {:?}", e);
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
