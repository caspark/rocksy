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
    debug_on: bool,
    _phantom_data: PhantomData<B>,
}

impl<C: Service, B> ReverseProxy<C, B> {
    /// Construct a reverse proxy that dispatches to the given client.
    pub fn new(
        client: C,
        remote_ip: Option<IpAddr>,
        targets: Vec<Target>,
        debug_on: bool,
    ) -> ReverseProxy<C, B> {
        ReverseProxy {
            client,
            remote_ip,
            targets,
            debug_on,
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
        if self.debug_on {
            println!("Received request is {:?}", &request);
        }

        let incoming = format!(
            "{method} {uri}",
            method = request.method(),
            uri = request.uri().to_string()
        );

        let proxied_request = self.create_proxied_request(request);
        if let Some(target) = self.determine_target(&proxied_request) {
            if self.debug_on {
                println!("Determined target of {:?}", target);
            }
            let pointed_request = self.point_request_at_target(target, proxied_request);

            if self.debug_on {
                println!("Making a request of {:?}", &pointed_request);
            }

            // clone to allow moving target into closure
            let target = target.clone();
            Box::new(self.client.call(pointed_request).then(move |response| {
                Ok(match response {
                    Ok(response) => {
                        log_request_response(&incoming, target.name().as_ref(), response.status());
                        create_proxied_response(response)
                    }
                    Err(error) => {
                        eprintln!("Failed to proxy request to {:?}! {}", target, error);
                        Response::new().with_status(StatusCode::InternalServerError)
                    }
                })
            }))
        } else {
            // no valid target for this request - should respond with 404
            let response = Response::new().with_status(StatusCode::NotFound);
            log_request_response(&incoming, "Rocksy (fallback)", response.status());
            Box::new(Ok(response).into_future())
        }
    }
}

fn log_request_response(incoming: &str, responder: &str, status: StatusCode) {
    let status_name = match status.canonical_reason() {
        Some(reason) => format!(" {}", reason),
        None => "".to_owned(),
    };
    println!(
        "{incoming} -> {target_name} -> {status_code}{status_name}",
        incoming = incoming,
        target_name = responder,
        status_code = status.as_u16(),
        status_name = status_name
    );
}
