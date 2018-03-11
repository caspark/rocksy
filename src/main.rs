#[macro_use]
extern crate clap;
extern crate futures;
#[macro_use]
extern crate hyper;
#[macro_use]
extern crate lazy_static;
extern crate regex;
extern crate tokio_core;
extern crate unicase;

mod config;
mod proxy;

use clap::{App, AppSettings, Arg};
use futures::Stream;
use hyper::Client;
use hyper::server::Http;
use proxy::ReverseProxy;
use config::{parse_target, Target};
use std::error::Error;
use std::net::SocketAddr;
use tokio_core::net::TcpListener;
use tokio_core::reactor::Core;

fn run(config: Config) -> hyper::Result<()> {
    //FIXME update output when logic is changed
    println!(
        "Listening on {} and proxying to first of {:?}",
        &config.listen_addr, &config.targets
    );

    // Set up the Tokio reactor core
    let mut core = Core::new()?;
    let handle = core.handle();

    // Listen to incoming requests over TCP, and forward them to a new `ReverseProxy`
    let listener = TcpListener::bind(&config.listen_addr, &handle)?;
    let http = Http::new();
    let server = listener.incoming().for_each(|(socket, addr)| {
        if config.debug {
            println!(
                "Received TCP connect with socket {:?} and address {:?}",
                socket, addr
            )
        }
        let client = Client::new(&handle);

        let service = ReverseProxy::new(client, Some(addr.ip()), config.targets.clone());
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

#[derive(Clone, Debug)]
struct Config {
    debug: bool,
    listen_addr: SocketAddr,
    targets: Vec<Target>,
}

fn is_valid_target(v: String) -> Result<(), String> {
    parse_target(v).map(|_| ())
}

fn main() {
    let matches = App::new("rocksy")
        .about(
            "A development HTTP reverse proxy for consolidating multiple services onto one port.",
        )
        .version(crate_version!())
        .setting(AppSettings::UnifiedHelpMessage)
        .arg(Arg::with_name("targets")
            .long("target")
            .short("t")
            .index(1)
            .help("Add a target to proxy requests to (with optional regular expression matching on path)")
            .validator(is_valid_target)
            .required(true)
            .multiple(true)
        )
        .arg(
            Arg::with_name("port")
                .long("port")
                .short("p")
                .value_name("PORT")
                .help("Sets the port that Rocksy should listen on")
                .default_value("5555")
                .validator(is_valid_port)
        )
        .arg(
            Arg::with_name("interface")
                .long("interface")
                .short("i")
                .value_name("INTERFACE")
                .help("Sets the network interface that Rocksy should listen on")
                .default_value("127.0.0.1")
                .validator(is_valid_interface)
        )
        .arg(
            Arg::with_name("debug")
                .long("debug")
                .short("d")
                .help("Switches on extra debug logging")
        )
        .get_matches();

    let debug_on = matches.is_present("debug");
    if debug_on {
        println!("Parsed command line arguments of: {:?}", matches);
    }

    let listen_addr = format!(
        "{}:{}",
        matches.value_of("interface").expect("has default value"),
        matches.value_of("port").expect("has default value")
    ).parse::<SocketAddr>()
        .expect("interface and port should be valid");

    let targets = matches
        .values_of("targets")
        .expect("targets is a required argument")
        .map(|raw| parse_target(raw).expect("target should already be validated"))
        .collect();

    let config = Config {
        debug: debug_on,
        listen_addr,
        targets,
    };

    if debug_on {
        println!("Running with config of: {:?}", &config);
    }

    if let Err(error) = run(config) {
        eprintln!("Fatal error: {}", error);
        std::process::exit(1);
    }
}
