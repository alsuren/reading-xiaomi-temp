use anyhow::{anyhow, Context};
use btleplug::api::{Central, CentralEvent};
use btleplug::bluez::manager::Manager;
use mijia::{connect_and_subscribe, hashmap_from_file, FailureCompat};
use std::{
    collections::VecDeque,
    sync::Arc,
    time::{Duration, Instant},
};

fn main() -> anyhow::Result<()> {
    let sensor_names = Arc::new(hashmap_from_file("sensor_names.conf")?);

    let manager = Manager::new().unwrap();
    let adapter = manager
        .adapters()
        .compat()?
        .into_iter()
        .nth(0)
        .ok_or(anyhow!("no adaptors"))?;
    // power-cycle the adaptor on startup for predictable results, and to prevent
    // interference from the bluez dbus daemon.
    manager.down(&adapter).compat()?;
    manager.up(&adapter).compat()?;
    let central = adapter.connect().compat()?;
    let event_receiver = central.event_receiver().unwrap();

    println!("Scanning");
    central.filter_duplicates(false);
    central.active(true);
    central.start_scan().compat().context("starting scan")?;

    let print_sensor_readings = {
        let sensor_names = sensor_names.clone();
        move |bd_addr, readings| {
            println!(
                "{} {} ({:?})",
                bd_addr,
                readings,
                sensor_names.get(&bd_addr).map_or("unnamed", String::as_str)
            )
        }
    };

    let mut sensors_to_connect = VecDeque::new();
    loop {
        let start = Instant::now();
        let next_timeout = start + Duration::from_secs(5);
        while let Ok(event) = event_receiver.recv_timeout(Duration::from_secs(5)) {
            if let CentralEvent::DeviceDiscovered(bd_addr) = event {
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
            connect_and_subscribe(&central, bd_addr, print_sensor_readings.clone())
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
