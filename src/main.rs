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
use std::net::SocketAddr;
use std::error::Error;

fn run(config: Config) -> hyper::Result<()> {
    println!(
        "Listening on {} and proxying to first of {:?}" //FIXME update output when logic is changed,
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

        //FIXME we should pass in a function which returns the correct Target, then pass that in
        let target = config
            .targets
            .first()
            .clone()
            .expect("at least 1 target is guaranteed")
            .address
            .clone();

        let service = ReverseProxy::new(client, Some(addr.ip()), target);
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

#[derive(Clone, Debug, PartialEq, Eq)]
struct Target {
    name: String,
    address: String,
    //FIXME this should be of type regex
    pattern: Option<String>,
}

impl Target {
    fn new<S: Into<String>>(name: S, address: S, pattern: Option<S>) -> Target {
        Target {
            name: name.into(),
            address: address.into(),
            pattern: pattern.map(Into::into),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct Config {
    debug: bool,
    listen_addr: SocketAddr,
    targets: Vec<Target>,
}

fn parse_target<S: Into<String>>(v: S) -> Result<Target, String> {
    // expected format is "name at target_url if regex_pattern"
    // only target_url is strictly required
    let literal_at = " at ";
    let literal_if = " if ";

    let mut name = None;
    let mut address = v.into();
    if let Some(at_pos) = address.find(literal_at) {
        name = Some(address[0..at_pos].into());
        address = address[at_pos + literal_at.len()..].into();
    }

    let mut pattern = None;
    if let Some(if_pos) = address.find(literal_if) {
        pattern = Some(address[if_pos + literal_if.len()..].into());
        address = address[0..if_pos].into();
    }

    Ok(Target::new(
        name.unwrap_or(address.clone()),
        address,
        pattern,
    ))
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_target_with_everything_succeeds() {
        let t = parse_target("backend at http://127.0.0.1:9000 if ^/api.*$").unwrap();

        assert_eq!(
            t,
            Target::new("backend", "http://127.0.0.1:9000", Some("^/api.*$"))
        );
    }

    #[test]
    fn parse_target_with_no_name_succeeds() {
        let t = parse_target("http://127.0.0.1:9000 if ^/api.*$").unwrap();

        assert_eq!(
            t,
            Target::new(
                "http://127.0.0.1:9000",
                "http://127.0.0.1:9000",
                Some("^/api.*$")
            )
        );
    }

    #[test]
    fn parse_target_with_no_pattern_succeeds() {
        let t = parse_target("backend at http://127.0.0.1:9000").unwrap();

        assert_eq!(t, Target::new("backend", "http://127.0.0.1:9000", None));
    }

    #[test]
    fn parse_target_with_neither_name_nor_pattern_succeeds() {
        let t = parse_target("http://127.0.0.1:9000").unwrap();

        assert_eq!(
            t,
            Target::new("http://127.0.0.1:9000", "http://127.0.0.1:9000", None)
        );
    }
}
