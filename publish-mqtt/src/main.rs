use anyhow::{anyhow, Context};
use btleplug::api::Central;
use futures::FutureExt;
use mijia::{connect_and_subscribe, hashmap_from_file, FailureCompat, Readings};
use rumqttc::MqttOptions;
use rustls::ClientConfig;
use std::collections::{HashMap, VecDeque};
use std::error::Error;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::{task, try_join};

mod homie;

use btleplug::{
    api::{BDAddr, CentralEvent},
    bluez::{adapter::ConnectedAdapter, manager::Manager},
};
use homie::{Datatype, HomieDevice, Node, Property};

const DEFAULT_MQTT_PREFIX: &str = "homie";
const DEFAULT_DEVICE_ID: &str = "mijia-bridge";
const DEFAULT_DEVICE_NAME: &str = "Mijia bridge";
const DEFAULT_HOST: &str = "test.mosquitto.org";
const DEFAULT_PORT: u16 = 1883;
const INCOMING_TIMEOUT_MS: u64 = 1_000;
const SENSOR_NAMES_FILENAME: &str = "sensor_names.conf";

#[tokio::main(core_threads = 2)]
async fn main() -> Result<(), Box<dyn Error + Send + Sync>> {
    dotenv::dotenv()?;
    pretty_env_logger::init();
    color_backtrace::install();

    let device_id = std::env::var("DEVICE_ID").unwrap_or_else(|_| DEFAULT_DEVICE_ID.to_string());
    let device_name =
        std::env::var("DEVICE_NAME").unwrap_or_else(|_| DEFAULT_DEVICE_NAME.to_string());
    let client_name = std::env::var("CLIENT_NAME").unwrap_or_else(|_| device_id.clone());

    let host = std::env::var("HOST").unwrap_or_else(|_| DEFAULT_HOST.to_string());

    let port = std::env::var("PORT")
        .ok()
        .and_then(|val| val.parse::<u16>().ok())
        .unwrap_or(DEFAULT_PORT);

    let mut mqttoptions = MqttOptions::new(client_name, host, port);

    let username = std::env::var("USERNAME").ok();
    let password = std::env::var("PASSWORD").ok();

    mqttoptions.set_keep_alive(5);
    if let (Some(u), Some(p)) = (username, password) {
        mqttoptions.set_credentials(u, p);
    }

    // Use `env -u USE_TLS` to unset this variable if you need to clear it.
    if std::env::var("USE_TLS").is_ok() {
        let mut client_config = ClientConfig::new();
        client_config.root_store =
            rustls_native_certs::load_native_certs().expect("could not load platform certs");
        mqttoptions.set_tls_client_config(Arc::new(client_config));
    }

    let mqtt_prefix =
        std::env::var("MQTT_PREFIX").unwrap_or_else(|_| DEFAULT_MQTT_PREFIX.to_string());
    let device_base = format!("{}/{}", mqtt_prefix, device_id);
    let (homie, mqtt_handle) = HomieDevice::spawn(&device_base, &device_name, mqttoptions).await;

    let local = task::LocalSet::new();

    // FIXME: this no longer needs to be a spawn_local, because the new bluetooth
    // stack is thread safe.
    let bluetooth_handle = local.spawn_local(async move {
        bluetooth_mainloop(homie).await.unwrap();
    });

    // Poll everything to completion, until the first one bombs out.
    let res: Result<_, Box<dyn Error + Send + Sync>> = try_join! {
        // LocalSet finished first. Colour me confused.
        local.map(|()| Ok(println!("WTF?"))),
        // Bluetooth finished first. Convert error and get on with your life.
        bluetooth_handle.map(|res| Ok(res?)),
        // MQTT event loop finished first.
        // Unwrap the JoinHandle Result to get to the real Result.
        mqtt_handle.map(|res| Ok(res??)),
    };
    res?;
    Ok(())
}

fn node_id_name_for_sensor(
    mac_address: &BDAddr,
    sensor_names: &HashMap<BDAddr, String>,
) -> (String, String) {
    let mac_address_string = mac_address.to_string();
    let node_id = mac_address_string.replace(":", "");
    let node_name = sensor_names
        .get(&mac_address)
        .cloned()
        .unwrap_or(mac_address_string);
    (node_id, node_name)
}

async fn connect_start_sensor<'a>(
    central: &ConnectedAdapter,
    homie: &mut HomieDevice,
    sensor_names: &HashMap<BDAddr, String>,
    properties: Vec<Property>,
    sensor: &BDAddr,
    tx: async_channel::Sender<(BDAddr, Readings)>,
) -> Result<(), Box<dyn Error>> {
    // FIXME: decide whether I want to pass around &BDAddr or BDAddr everywhere.
    // BDAddr implements copy, so &BDAddr feels a bit pointless.
    connect_and_subscribe(central, *sensor, move |mac_address, readings| {
        tx.try_send((mac_address, readings))
            .expect("tx should be unbounded");
    })?;

    let (node_id, node_name) = node_id_name_for_sensor(sensor, sensor_names);
    homie
        .add_node(Node::new(
            node_id,
            node_name,
            "Mijia sensor".to_string(),
            properties.to_vec(),
        ))
        .await?;
    Ok(())
}

async fn report_readings(
    homie: &HomieDevice,
    sensor_names: &HashMap<BDAddr, String>,
    mac_address: BDAddr,
    readings: Readings,
) -> anyhow::Result<()> {
    let Readings {
        temperature,
        humidity,
        battery_voltage,
        battery_percent,
    } = readings;
    let (node_id, name) = node_id_name_for_sensor(&mac_address, &sensor_names);

    println!(
        "{} Temperature: {:.2}ºC Humidity: {:?}% Battery {} mV ({} %) ({})",
        mac_address, temperature, humidity, battery_voltage, battery_percent, name
    );

    homie
        .publish_value(&node_id, "temperature", format!("{:.2}", temperature))
        .await?;
    homie.publish_value(&node_id, "humidity", humidity).await?;
    homie
        .publish_value(&node_id, "battery", battery_percent)
        .await?;

    Ok(())
}

async fn bluetooth_mainloop(mut homie: HomieDevice) -> anyhow::Result<()> {
    let sensor_names = hashmap_from_file(SENSOR_NAMES_FILENAME)?;

    homie.start().await?;

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

    // FIXME: push this down into the place where it's used.
    let properties = [
        Property::new("temperature", "Temperature", Datatype::Float, Some("ºC")),
        Property::new("humidity", "Humidity", Datatype::Integer, Some("%")),
        Property::new("battery", "Battery level", Datatype::Integer, Some("%")),
    ];

    let mut sensors_to_connect: VecDeque<BDAddr> = Default::default();
    let mut sensors_connected: Vec<BDAddr> = vec![];
    let (tx, rx) = async_channel::unbounded();

    homie.ready().await?;

    loop {
        println!(
            "{} sensors connected and {} sensors in queue to connect.",
            sensors_connected.len(),
            sensors_to_connect.len()
        );
        // Try to connect to a single sensor from the front of the queue.
        // FIXME: connecting to a sensor is currently a blocking operation with
        // no timeout.
        if let Some(mac_address) = sensors_to_connect.pop_front() {
            let name = sensor_names
                .get(&mac_address)
                .map_or("unnamed", String::as_str);
            println!("Trying to connect to {}", name);
            match connect_start_sensor(
                &central,
                &mut homie,
                &sensor_names,
                properties.to_vec(),
                &mac_address,
                tx.clone(),
            )
            .await
            {
                Err(e) => {
                    println!("Failed to connect to {} ({}): {:?}", mac_address, name, e);
                    sensors_to_connect.push_back(mac_address);
                }
                Ok(()) => {
                    println!(
                        "Connected to {} ({}) and started notifications",
                        mac_address, name
                    );
                    sensors_connected.push(mac_address);
                }
            }
        }

        // Process events until there are none available for the timeout.
        // Should poll for between INCOMING_TIMEOUT_MS and 2*INCOMING_TIMEOUT_MS
        let recv_until = Instant::now() + Duration::from_millis(INCOMING_TIMEOUT_MS);
        while let Ok(event) =
            event_receiver.recv_timeout(Duration::from_millis(INCOMING_TIMEOUT_MS))
        {
            match event {
                CentralEvent::DeviceDiscovered(mac_address) => {
                    if !sensors_to_connect.contains(&mac_address)
                        && !sensors_connected.contains(&mac_address)
                        && sensor_names.contains_key(&mac_address)
                    {
                        println!(
                            "Enqueueing {:?} {:?}",
                            mac_address,
                            sensor_names.get(&mac_address).unwrap()
                        );
                        sensors_to_connect.push_back(mac_address);
                    }
                }
                CentralEvent::DeviceDisconnected(mac_address) => {
                    if let Some(sensor_index) =
                        sensors_connected.iter().position(|s| s == &mac_address)
                    {
                        let sensor = sensors_connected.remove(sensor_index);
                        let (node_id, node_name) = node_id_name_for_sensor(&sensor, &sensor_names);
                        println!("{} disconnected", node_name);
                        homie.remove_node(&node_id).await?;
                        sensors_to_connect.push_back(sensor);
                    } else {
                        println!(
                            "{} disconnected but wasn't known to be connected.",
                            mac_address
                        );
                    }
                }
                _ => {
                    log::trace!("{:?}", event);
                }
            };
            if Instant::now() > recv_until {
                break;
            };
        }
        // FIXME: This should go on its own mainloop, and use rx.recv().
        // For it should be okay, because it's just shovelling messages from
        // one channel to another.
        loop {
            match rx.try_recv() {
                Ok((mac_address, readings)) => {
                    report_readings(&homie, &sensor_names, mac_address, readings).await?;
                }
                Err(async_channel::TryRecvError::Empty) => break,
                Err(async_channel::TryRecvError::Closed) => {
                    anyhow::bail!("someone closed the channel")
                }
            }
        }
    }
}
