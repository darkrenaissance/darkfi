use std::net::SocketAddr;

use clap::{App, Arg};

use super::Channel;
use crate::{net::Settings, Result};

pub struct CliOption {
    pub network_settings: Settings,
    pub username: Option<String>,
    pub channel_name: Option<String>,
    pub new_channel: Option<Channel>,
    pub verbose: bool,
    pub log_path: Box<std::path::PathBuf>,
}

impl CliOption {
    pub fn get() -> Result<CliOption> {
        let mat = App::new("DarkPulse")
            .version("0.0.1")
            .author("Dark Renaissance Technologies")
            .about("An anonymous p2p chat application")
            .arg(
                Arg::new("accept")
                    .short('a')
                    .value_name("ADDRESSES")
                    .help("accept address")
                    .long("accept")
                    .required(true),
            )
            .arg(Arg::new("slots").value_name("SLOTS").long("slots").help("Connection slots"))
            .arg(Arg::new("verbose").takes_value(false).long("verbose").help("increase verbosity"))
            .arg(
                Arg::new("connects")
                    .value_name("MANUAL_CONNECTS")
                    .multiple_occurrences(true)
                    .takes_value(true)
                    .short('c')
                    .long("connects")
                    .help("Manual connections"),
            )
            .arg(
                Arg::new("seed")
                    .value_name("ADDRESSES")
                    .multiple_occurrences(true)
                    .takes_value(true)
                    .short('s')
                    .long("seed")
                    .help("Connect to the seed node"),
            )
            .arg(
                Arg::new("log")
                    .value_name("LOG_PATH")
                    .takes_value(true)
                    .long("log")
                    .help("Log file path"),
            )
            .arg(
                Arg::new("username")
                    .value_name("USERNAME")
                    .short('u')
                    .long("username")
                    .help("node's username"),
            )
            .arg(
                Arg::new("channel")
                    .value_name("CHANNEL")
                    .short('h')
                    .long("channel")
                    .help("switch to one of available channels"),
            )
            .subcommand(
                App::new("newchannel")
                    .about("add new channel")
                    .arg(
                        Arg::new("name")
                            .long("channelname")
                            .required(true)
                            .value_name("CHANNELNAME")
                            .help("name for the new channel"),
                    )
                    .arg(
                        Arg::new("address")
                            .long("channeladdress")
                            .required(true)
                            .value_name("CHANNELADDRESS")
                            .help("address for the new channel"),
                    ),
            )
            .get_matches();

        let mut accept_addr: Option<SocketAddr> = None;
        if let Some(addr) = mat.value_of("accept") {
            accept_addr = Some(addr.parse()?);
        }

        let mut connection_slots = 0;
        if let Some(slots) = mat.value_of("slots") {
            connection_slots = slots.parse()?;
        };

        let mut seed_addresses: Vec<SocketAddr> = vec![];
        if let Some(seed_addrs) = mat.values_of("seed") {
            seed_addresses = Self::collect_addrs(seed_addrs.collect::<Vec<&str>>());
        };

        let mut manual_connects: Vec<SocketAddr> = vec![];
        if let Some(man_connects) = mat.values_of("connects") {
            manual_connects = Self::collect_addrs(man_connects.collect::<Vec<&str>>());
        };

        let mut username = None;

        if let Some(uname) = mat.value_of("username") {
            username = Some(String::from(uname));
        }

        let mut channel_name = None;

        if let Some(chan) = mat.value_of("channel") {
            channel_name = Some(String::from(chan));
        }

        let mut new_channel: Option<Channel> = None;

        if let Some(newch) = mat.subcommand_matches("newchannel") {
            let mut new_channel_name = String::new();
            let mut new_channel_address = String::new();
            if let Some(channelname) = newch.value_of("name") {
                new_channel_name = String::from(channelname);
            }
            if let Some(channeladdress) = newch.value_of("address") {
                new_channel_address = String::from(channeladdress);
            }
            new_channel = Some(Channel::gen_new_with_addr(new_channel_name, new_channel_address)?);
        }

        let verbose = mat.is_present("verbose");

        let log_path = Box::new(
            if let Some(log_path) = mat.value_of("log") {
                std::path::Path::new(log_path)
            } else {
                std::path::Path::new("/tmp/darkpulsenode.log")
            }
            .to_path_buf(),
        );

        let network_settings = Settings {
            inbound: accept_addr,
            outbound_connections: connection_slots,
            seed_query_timeout_seconds: 8,
            connect_timeout_seconds: 10,
            channel_handshake_seconds: 4,
            channel_heartbeat_seconds: 10,
            external_addr: accept_addr,
            peers: manual_connects,
            seeds: seed_addresses,
            manual_attempt_limit: 10,
        };

        Ok(CliOption { network_settings, username, channel_name, new_channel, verbose, log_path })
    }

    fn collect_addrs(addrs: Vec<&str>) -> Vec<SocketAddr> {
        let addrs: Vec<SocketAddr> = addrs
            .iter()
            .map(|addr| {
                let addr: SocketAddr = addr.parse().expect("unable to parse on of the addresses");
                addr
            })
            .collect();

        addrs
    }
}
