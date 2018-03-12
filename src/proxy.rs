//! A simple reverse proxy, to be used with [Hyper] and [Tokio].
//!
//! The implementation ensures that [Hop-by-hop headers] are stripped correctly in both directions,
//! and adds the client's IP address to a comma-space-separated list of forwarding addresses in the
//! `X-Forwarded-For` header.
//!
//! The implementation is based on Go's [`httputil.ReverseProxy`].
//!
//! [Hyper]: http://hyper.rs/
//! [Tokio]: http://tokio.rs/
//! [Hop-by-hop headers]: http://www.w3.org/Protocols/rfc2616/rfc2616-sec13.html
//! [`httputil.ReverseProxy`]: https://golang.org/pkg/net/http/httputil/#ReverseProxy
//!
//! # Example
//!
//! Because the reverse proxy needs client, we also need a [`Handle`]. This
//! means that we can't take advantage of hyper's handy [`Http::bind`] method
//! to quickly set up the server. Instead we do things manually:
//!
//! [`Handle`]: https://docs.rs/tokio-core/*/tokio_core/reactor/struct.Handle.html
//! [`Http::bind`]: https://docs.rs/hyper/*/hyper/server/struct.Http.html#method.bind
//!
//! ```rust,no_run
//! extern crate futures;
//! extern crate hyper;
//! extern crate hyper_reverse_proxy;
//! extern crate tokio_core;
//!
//! use futures::Stream;
//! use hyper::Client;
//! use hyper::server::Http;
//! use hyper_reverse_proxy::ReverseProxy;
//! use tokio_core::net::TcpListener;
//! use tokio_core::reactor::Core;
//! use std::net::{SocketAddr, Ipv4Addr};
//!
//! fn run() -> hyper::Result<()> {
//!     // Set up the Tokio reactor core
//!     let mut core = Core::new()?;
//!     let handle = core.handle();
//!
//!     // Set up a TCP socket to listen to
//!     let listen_addr = SocketAddr::new(Ipv4Addr::new(127, 0, 0, 1).into(), 8080);
//!     let listener = TcpListener::bind(&listen_addr, &handle)?;
//!
//!     // Listen to incoming requests over TCP, and forward them to a new `ReverseProxy`
//!     let http = Http::new();
//!     let server = listener.incoming().for_each(|(socket, addr)| {
//!         let client = Client::new(&handle);
//!         let service = ReverseProxy::new(client, Some(addr.ip()));
//!         http.bind_connection(&handle, socket, addr, service);
//!         Ok(())
//!     });
//!
//!     // Start our server on the reactor core
//!     core.run(server)?;
//!
//!     Ok(())
//! }
//!
//! fn main() {
//!     use std::io::{self, Write};
//!
//!     if let Err(error) = run() {
//!         write!(&mut io::stderr(), "{}", error).expect("Error writing to stderr");
//!         std::process::exit(1);
//!     }
//! }
//! ```
//!
//! Note that in a production system we might also want to:
//!
//! - listen for `SIGINT` and `SIGTERM` signals using [tokio_signal]
//! - shut down the server gracefully, waiting for outstanding requests to complete
//!
//! To see an example of this, look at [examples/extended.rs].
//!
//! [examples/extended.rs]: https://github.com/brendanzab/hyper-reverse-proxy/blob/master/examples/extended.rs
//! [tokio_signal]: https://github.com/alexcrichton/tokio-signal

use config::Target;
use futures::future::Future;
use futures::IntoFuture;
use hyper;
use hyper::{Body, Headers, Request, Response, StatusCode, Uri};
use hyper::header::Host as HostHeader;
use hyper::server::Service;
use std::marker::PhantomData;
use std::net::IpAddr;

fn is_hop_header(name: &str) -> bool {
    use unicase::Ascii;

    // A list of the headers, using `unicase` to help us compare without
    // worrying about the case, and `lazy_static!` to prevent reallocation
    // of the vector.
    lazy_static! {
        static ref HOP_HEADERS: Vec<Ascii<&'static str>> = vec![
            Ascii::new("Connection"),
            Ascii::new("Keep-Alive"),
            Ascii::new("Proxy-Authenticate"),
            Ascii::new("Proxy-Authorization"),
            Ascii::new("Te"),
            Ascii::new("Trailers"),
            Ascii::new("Transfer-Encoding"),
            Ascii::new("Upgrade"),
        ];
    }

    HOP_HEADERS.iter().any(|h| h == &name)
}

/// Returns a clone of the headers without the [hop-by-hop headers].
///
/// [hop-by-hop headers]: http://www.w3.org/Protocols/rfc2616/rfc2616-sec13.html
fn remove_hop_headers(headers: &Headers) -> Headers {
    headers
        .iter()
        .filter(|header| !is_hop_header(header.name()))
        .collect()
}

// TODO: use https://developer.mozilla.org/en-US/docs/Web/HTTP/Headers/Forwarded ?
header! {
    /// `X-Forwarded-For` header.
    ///
    /// The `X-Forwarded-For` header describes the path of
    /// proxies this request has been forwarded through.
    ///
    /// # Example Values
    ///
    /// * `2001:db8:85a3:8d3:1319:8a2e:370:7348`
    /// * `203.0.113.195`
    /// * `203.0.113.195, 70.41.3.18, 150.172.238.178`
    ///
    /// # Examples
    ///
    /// ```
    /// # extern crate hyper;
    /// # extern crate hyper_reverse_proxy;
    /// use hyper::Headers;
    /// use hyper_reverse_proxy::XForwardedFor;
    /// use std::net::{Ipv4Addr, Ipv6Addr};
    ///
    /// # fn main() {
    /// let mut headers = Headers::new();
    /// headers.set(XForwardedFor(vec![
    ///     Ipv4Addr::new(127, 0, 0, 1).into(),
    ///     Ipv6Addr::new(0, 0, 0, 0, 0, 0, 0, 1).into(),
    /// ]));
    /// # }
    /// ```
    ///
    /// # References
    ///
    /// - [MDN](https://developer.mozilla.org/en-US/docs/Web/HTTP/Headers/X-Forwarded-For)
    /// - [Wikipedia](https://en.wikipedia.org/wiki/X-Forwarded-For)
    (XForwardedFor, "X-Forwarded-For") => (IpAddr)+

    // test_x_forwarded_for {
    //     // Testcases from MDN
    //     test_header!(test1, vec![b"2001:db8:85a3:8d3:1319:8a2e:370:7348"]);
    //     test_header!(test2, vec![b"203.0.113.195"]);
    //     test_header!(test3, vec![b"203.0.113.195, 70.41.3.18, 150.172.238.178"]);
    // }
}

fn create_proxied_response<B>(mut response: Response<B>) -> Response<B> {
    *response.headers_mut() = remove_hop_headers(response.headers());
    response
}

/// A `Service` that takes an incoming request, sends it to a given `Client`, then proxies back
/// the response.
pub struct ReverseProxy<C: Service, B = Body> {
    client: C,
    remote_ip: Option<IpAddr>,
    targets: Vec<Target>,
    _phantom_data: PhantomData<B>,
}

impl<C: Service, B> ReverseProxy<C, B> {
    /// Construct a reverse proxy that dispatches to the given client.
    pub fn new(client: C, remote_ip: Option<IpAddr>, targets: Vec<Target>) -> ReverseProxy<C, B> {
        ReverseProxy {
            client,
            remote_ip,
            targets,
            _phantom_data: PhantomData,
        }
    }

    fn create_proxied_request(&self, mut request: Request<B>) -> Request<B> {
        *request.headers_mut() = remove_hop_headers(request.headers());

        // Add forwarding information in the headers
        if let Some(ip) = self.remote_ip {
            // This is kind of ugly because of borrowing. Maybe hyper's `Headers` object
            // could use an entry API like `std::collections::HashMap`?
            if request.headers().has::<XForwardedFor>() {
                if let Some(prior) = request.headers_mut().get_mut::<XForwardedFor>() {
                    prior.push(ip);
                }
            } else {
                let header = XForwardedFor(vec![ip]);
                request.headers_mut().set(header);
            }
        }

        request
    }

    fn determine_target(&self, request: &Request<B>) -> Option<&Target> {
        self.targets
            .iter()
            .find(|&t| t.valid_for(request.uri().path()))
    }

    fn point_request_at_target(&self, target: &Target, mut request: Request<B>) -> Request<B> {
        let mut target_uri = target.address().to_owned();

        target_uri.push_str(request.uri().path());
        if let Some(query) = request.uri().query() {
            target_uri.push_str("?");
            target_uri.push_str(query);
        }

        if let Some(target) = target_uri.parse::<Uri>().ok() {
            request.headers_mut().remove::<HostHeader>();
            request.set_uri(target);
        } else {
            eprintln!("Failed to build request url for {:?}", &request)
        }

        request
    }
}

impl<C, B> Service for ReverseProxy<C, B>
where
    B: 'static,
    C: Service<Request = Request<B>, Response = Response<B>>,
    C::Error: 'static + ::std::fmt::Display,
    C::Future: 'static,
{
    type Request = Request<B>;
    type Response = Response<B>;
    type Error = hyper::Error;
    type Future = Box<Future<Item = Response<B>, Error = hyper::Error>>;

    fn call(&self, request: Self::Request) -> Self::Future {
        println!(">Received request is {:?}", &request);

        let proxied_request = self.create_proxied_request(request);
        if let Some(target) = self.determine_target(&proxied_request) {
            println!(">>Determined target of {:?}", target);
            let pointed_request = self.point_request_at_target(target, proxied_request);

            println!(">>>Making a request of {:?}", &pointed_request);
            Box::new(self.client.call(pointed_request).then(|response| {
                Ok(match response {
                    Ok(response) => create_proxied_response(response),
                    Err(error) => {
                        eprintln!("Error: {}", error); // TODO: Configurable logging
                        Response::new().with_status(StatusCode::InternalServerError)
                        // TODO: handle trailers
                    }
                })
            }))
        } else {
            // no valid target for this request - should respond with 404
            let r = Response::new().with_status(StatusCode::NotFound);
            Box::new(Ok(r).into_future())
        }
    }
}
