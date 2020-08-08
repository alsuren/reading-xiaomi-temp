use blurz::{BluetoothEvent, BluetoothGATTCharacteristic, BluetoothSession};
use btleplug::api::{Central, CentralEvent};
#[cfg(target_os = "linux")]
use btleplug::bluez::{adapter::ConnectedAdapter, manager::Manager};
#[cfg(target_os = "macos")]
use btleplug::corebluetooth::{adapter::Adapter, manager::Manager};
use mijia::{connect_sensors, decode_value, find_sensors, print_sensors, scan};
use std::thread;
use std::time::Duration;

mod explore_device;

// adapter retrieval works differently depending on your platform right now.
// API needs to be aligned.

#[cfg(target_os = "macos")]
fn get_central(manager: &Manager) -> Adapter {
    let adapters = manager.adapters().unwrap();
    adapters.into_iter().nth(0).unwrap()
}

#[cfg(target_os = "linux")]
fn get_central(manager: &Manager) -> ConnectedAdapter {
    let adapters = manager.adapters().unwrap();
    let adapter = adapters.into_iter().nth(0).unwrap();
    adapter.connect().unwrap()
}

fn main() {
    let manager = Manager::new().unwrap();
    let central = get_central(&manager);
    let event_receiver = central.event_receiver().unwrap();

    // FIXME: turn the bluetooth adapter on?
    println!("Scanning");
    central.start_scan().unwrap();

    println!("waiting");
    while let Ok(event) = event_receiver.recv() {
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
            e => {
                println!("Other event {:?}", e);
            }
        }
    }
    let bt_session = &BluetoothSession::create_session(None).unwrap();
    let device_list = scan(&bt_session).unwrap();
    let sensors = find_sensors(&bt_session, &device_list);
    println!();
    print_sensors(&sensors);
    let connected_sensors = connect_sensors(&sensors);
    print_sensors(&connected_sensors);

    // We need to wait a bit after calling connect to safely
    // get the gatt services
    thread::sleep(Duration::from_millis(5000));
    for device in connected_sensors {
        explore_device::explore_gatt_profile(bt_session, &device);
        let temp_humidity =
            BluetoothGATTCharacteristic::new(bt_session, device.get_id() + "/service0021/char0035");
        if let Err(e) = temp_humidity.start_notify() {
            println!("Failed to start notify on {}: {:?}", device.get_id(), e);
        }
    }

    println!("READINGS");
    loop {
        for event in BluetoothSession::create_session(None)
            .unwrap()
            .incoming(1000)
            .map(BluetoothEvent::from)
        {
            if event.is_none() {
                continue;
            }

            let (object_path, value) = match event.clone().unwrap() {
                BluetoothEvent::Value { object_path, value } => (object_path, value),
                _ => continue,
            };

            if let Some((temperature, humidity)) = decode_value(&value) {
                println!(
                    "{} Temperature: {:.2}ºC Humidity: {:?}%",
                    object_path, temperature, humidity
                );
            }
        }
    }
}
