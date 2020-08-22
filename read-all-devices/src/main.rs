use blurz::{BluetoothDevice, BluetoothEvent, BluetoothSession};
use btleplug::api::{Central, CentralEvent};
#[cfg(target_os = "linux")]
use btleplug::bluez::{adapter::ConnectedAdapter, manager::Manager};
#[cfg(target_os = "macos")]
use btleplug::corebluetooth::{adapter::Adapter, manager::Manager};
use mijia::{
    connect_sensors, decode_value, find_sensors, hashmap_from_file, print_sensors, scan,
    start_notify_sensors, SERVICE_CHARACTERISTIC_PATH,
};
use std::thread;
use std::time::Duration;

mod explore_device;

const SENSOR_NAMES_FILENAME: &str = "sensor_names.conf";

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
    let sensor_names = hashmap_from_file(SENSOR_NAMES_FILENAME).unwrap();

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
            CentralEvent::DeviceUpdated(bd_addr) => {
                println!("DeviceUpdated: {:?}", bd_addr);
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
    print_sensors(&sensors, &sensor_names);
    let connected_sensors = connect_sensors(&sensors);
    print_sensors(&connected_sensors, &sensor_names);

    // We need to wait a bit after calling connect to safely
    // get the gatt services
    thread::sleep(Duration::from_millis(5000));
    for device in &connected_sensors {
        explore_device::explore_gatt_profile(bt_session, &device);
    }
    start_notify_sensors(bt_session, &connected_sensors);

    println!("READINGS");
    loop {
        for event in bt_session.incoming(1000).map(BluetoothEvent::from) {
            let (object_path, value) = match event {
                Some(BluetoothEvent::Value { object_path, value }) => (object_path, value),
                _ => continue,
            };

            // TODO: Make this less hacky.
            if !object_path.ends_with(SERVICE_CHARACTERISTIC_PATH) {
                continue;
            }
            let device_path = &object_path[..object_path.len() - SERVICE_CHARACTERISTIC_PATH.len()];
            let device = BluetoothDevice::new(bt_session, device_path.to_string());

            if let Some((temperature, humidity, battery_voltage, battery_percent)) =
                decode_value(&value)
            {
                let mac_address = device.get_address().unwrap();
                let name = sensor_names.get(&mac_address).unwrap_or(&mac_address);
                println!(
                    "{} ({}) Temperature: {:.2}ºC Humidity: {:?}% Battery: {:?} mV ({:?}%)",
                    object_path, name, temperature, humidity, battery_voltage, battery_percent
                );
            }
        }
    }
}
