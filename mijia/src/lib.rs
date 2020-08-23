use anyhow::{bail, Context};
use btleplug::api::{BDAddr, Central, CentralEvent, Peripheral, UUID};
use btleplug::bluez::{adapter::ConnectedAdapter, manager::Manager};
use failure::Fail;
use std::cmp::max;
use std::collections::HashMap;
use std::convert::TryInto;
use std::error::Error;
use std::fs::File;
use std::io::{self, BufRead, BufReader, ErrorKind};
use std::{str::FromStr, time::Duration};

const SCAN_DURATION: Duration = Duration::from_millis(5000);
const CONNECT_TIMEOUT_MS: i32 = 10_000;

const MIJIA_SERVICE_DATA_UUID: &str = "0000fe95-0000-1000-8000-00805f9b34fb";
pub const SERVICE_CHARACTERISTIC_PATH: &str = "/service0021/char0035";
const CONNECTION_INTERVAL_CHARACTERISTIC_PATH: &str = "/service0021/char0045";
/// 500 in little-endian
const CONNECTION_INTERVAL_500_MS: [u8; 3] = [0xF4, 0x01, 0x00];

const READINGS_CHARACTERISTIC_ID: &str = "EB:E0:CC:C1:7A:0A:4B:0C:8A:1A:6F:F2:99:7D:A3:A6";

/// Just .compat() from failure::ResultExt
trait FailureCompat<T> {
    fn compat(self) -> anyhow::Result<T>;
}

impl<T, E> FailureCompat<T> for Result<T, E>
where
    E: failure::Fail,
{
    fn compat(self) -> anyhow::Result<T> {
        Ok(self.map_err(|err| err.compat())?)
    }
}

// make into singular version
pub fn print_sensor(device: &impl Peripheral, sensor_names: &HashMap<BDAddr, String>) {
    let mac_address = device.address();
    let name = sensor_names
        .get(&mac_address)
        .map(String::as_ref)
        .unwrap_or("");
    let props = device.properties();
    println!(
        "{}: {:?}, {} services, '{}'",
        mac_address,
        props.local_name.unwrap_or_default(),
        device.characteristics().len(),
        name
    );
}

// port
pub fn connect_sensor<'a>(peripheral: &impl Peripheral) -> anyhow::Result<()> {
    let bd_addr = peripheral.address();
    peripheral
        .connect()
        .map_err(|err| err.compat())
        .with_context(|| format!("connecting to {:?}", bd_addr))
}

// port, but wants on_notification callback?
pub fn start_notify_sensor<'a>(peripheral: &impl Peripheral) -> anyhow::Result<()> {
    let bd_addr = peripheral.address();

    peripheral
        .discover_characteristics()
        .compat()
        .context("discovering characteristics")?
        .iter()
        .find(|c| c.uuid == READINGS_CHARACTERISTIC_ID.parse().unwrap())
        .map(|c| {
            peripheral
                .subscribe(c)
                .compat()
                .context("subscribing to readings")
        })
        .transpose()?;

    // FIXME: port this code across:
    // let connection_interval = BluetoothGATTCharacteristic::new(
    //     bt_session,
    //     connected_sensor.get_id() + CONNECTION_INTERVAL_CHARACTERISTIC_PATH,
    // );
    // connection_interval.write_value(CONNECTION_INTERVAL_500_MS.to_vec(), None)?;

    peripheral.on_notification(Box::new(move |val| {
        // FIXME: replace with user-provided callback
        println!("on_notification: {:?} {:?}", bd_addr, val)
    }));

    Ok(())
}

// keep
pub fn decode_value(value: &[u8]) -> Option<(f32, u8, u16, u16)> {
    if value.len() != 5 {
        return None;
    }

    let mut temperature_array = [0; 2];
    temperature_array.clone_from_slice(&value[..2]);
    let temperature = i16::from_le_bytes(temperature_array) as f32 * 0.01;
    let humidity = value[2];
    let battery_voltage = u16::from_le_bytes(value[3..5].try_into().unwrap());
    let battery_percent = (max(battery_voltage, 2100) - 2100) / 10;
    Some((temperature, humidity, battery_voltage, battery_percent))
}

// keep
/// Read the given file of key-value pairs into a hashmap.
/// Returns an empty hashmap if the file doesn't exist, or an error if it is malformed.
pub fn hashmap_from_file(filename: &str) -> Result<HashMap<BDAddr, String>, anyhow::Error> {
    let mut map = HashMap::new();
    if let Ok(file) = File::open(filename) {
        for line in BufReader::new(file).lines() {
            let line = line?;
            let parts: Vec<&str> = line.splitn(2, '=').collect();
            if parts.len() != 2 {
                bail!("Invalid line '{}'", line);
            }
            map.insert(parts[0].parse::<BDAddr>().compat()?, parts[1].to_string());
        }
    }
    Ok(map)
}
