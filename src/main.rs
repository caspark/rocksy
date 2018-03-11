#[macro_use]
extern crate clap;
extern crate futures;
#[macro_use]
extern crate hyper;
#[macro_use]
extern crate lazy_static;
extern crate tokio_core;
extern crate unicase;

mod proxy;

use clap::{App, AppSettings, Arg};
use futures::Stream;
use hyper::Client;
use hyper::server::Http;
use proxy::ReverseProxy;
use tokio_core::net::TcpListener;
use tokio_core::reactor::Core;
use std::net::{SocketAddr, Ipv4Addr};
use std::error::Error;

fn run(config: Config) -> hyper::Result<()> {
    println!("Listening on {} and proxying to {}", &config.listen_addr, &config.target);

    // Set up the Tokio reactor core
    let mut core = Core::new()?;
    let handle = core.handle();

    // Listen to incoming requests over TCP, and forward them to a new `ReverseProxy`
    let listener = TcpListener::bind(&config.listen_addr, &handle)?;
    let http = Http::new();
    let server = listener.incoming().for_each(|(socket, addr)| {
        let client = Client::new(&handle);
        let service = ReverseProxy::new(client, Some(addr.ip()), config.target.clone());
        http.bind_connection(&handle, socket, addr, service);
        Ok(())
    });

    // Start our server on the reactor core
    core.run(server)?;

    Ok(())
}

fn is_valid_port(v: String) -> Result<(), String> {
    match v.parse::<u16>() {
        Ok(_) => Ok(()),
        Err(e) => Err(format!(
            "{} (specify a number between {} and {})",
            e.description(),
            std::u16::MIN,
            std::u16::MAX
        )),
    }
}

fn is_valid_interface(v: String) -> Result<(), String> {
    use std::net::IpAddr;

    match v.parse::<IpAddr>() {
        Ok(_) => Ok(()),
        Err(e) => Err(format!(
            "{} (specify a valid network interface)",
            e.description()
        )),
    }
}

struct Config {
    debug: bool,
    listen_addr: SocketAddr,
    target: String,
}

fn main() {
    let matches = App::new("rocksy")
        .about(
            "A development HTTP reverse proxy for consolidating multiple services onto one port.",
        )
        .version(crate_version!())
        .setting(AppSettings::UnifiedHelpMessage)
        .arg(
            Arg::with_name("port")
                .long("port")
                .short("p")
                .value_name("PORT")
                .help("Sets the port that Rocksy should listen on")
                .default_value("5555")
                .validator(is_valid_port)
                .takes_value(true),
        )
        .arg(
            Arg::with_name("interface")
                .long("interface")
                .short("i")
                .value_name("NETWORK_INTERFACE")
                .help("Sets the network interface that Rocksy should listen on")
                .default_value("127.0.0.1")
                .validator(is_valid_interface)
                .takes_value(true),
        )
        .arg(
            Arg::with_name("debug")
                .long("debug")
                .short("d")
                .help("Switches on extra debug logging"),
        )
        .get_matches();

    let debug_on = matches.is_present("debug");
    if debug_on {
        println!("Parsed command line arguments of: {:?}", matches);
    }

    let listen_addr = format!("{}:{}",
                       matches.value_of("interface").expect("has default value"),
                       matches.value_of("port").expect("has default value"))
        .parse::<SocketAddr>().expect("interface and port should be valid");

    let config = Config {
        debug: debug_on,
        listen_addr,
        target: "http://127.0.0.1:8000".to_owned()
    };

    if let Err(error) = run(config) {
        eprintln!("Fatal error: {}", error);
        std::process::exit(1);
    }
}
