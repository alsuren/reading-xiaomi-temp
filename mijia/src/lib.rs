use anyhow::{bail, Context};
use btleplug::api::{BDAddr, Peripheral};
use failure::Fail;
use std::cmp::max;
use std::collections::HashMap;
use std::convert::TryInto;
use std::fs::File;
use std::io::{BufRead, BufReader};

// FIXME: discover devices by whether their service data has this in?
// currently we are discovering them by name.
// const MIJIA_SERVICE_DATA_UUID: &str = "0000fe95-0000-1000-8000-00805f9b34fb";

/// 500 in little-endian
const CONNECTION_INTERVAL_500_MS: [u8; 3] = [0xF4, 0x01, 0x00];

const READINGS_CHARACTERISTIC_ID: &str = "EB:E0:CC:C1:7A:0A:4B:0C:8A:1A:6F:F2:99:7D:A3:A6";
const INTERVAL_CHARACTERISTIC_ID: &str = "ebe0ccd8-7a0a-4b0c-8a1a-6ff2997da3a6";

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

    let characteristics = peripheral
        .discover_characteristics()
        .compat()
        .context("discovering characteristics")?;

    let readings_characteristic = characteristics
        .iter()
        .find(|c| c.uuid == READINGS_CHARACTERISTIC_ID.parse().unwrap())
        .ok_or(anyhow::anyhow!(
            "could not find readings characteristic on {:}",
            bd_addr
        ))?;

    peripheral
        .subscribe(readings_characteristic)
        .compat()
        .context("subscribing to readings")?;

    let interval_characteristic = characteristics
        .iter()
        .find(|c| c.uuid == INTERVAL_CHARACTERISTIC_ID.parse().unwrap())
        .ok_or(anyhow::anyhow!(
            "could not find interval characteristic on {:}",
            bd_addr
        ))?;
    peripheral
        .command(interval_characteristic, &CONNECTION_INTERVAL_500_MS)
        .compat()?;

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
