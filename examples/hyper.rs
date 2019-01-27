use failure::{Compat, Fail};
use futures::prelude::*;
use hyper::{
    client::connect::{Connect, Connected, Destination},
    Client, Uri,
};
use std::io::{prelude::*, stdout};
use std::net::SocketAddr;
use tokio_socks::{tcp::Socks5Stream, Error};
use tokio_tcp::TcpStream;

struct Connector {
    proxy: SocketAddr,
}

impl Connect for Connector {
    type Transport = TcpStream;
    type Error = Compat<Error>;
    type Future = Box<Future<Item = (Self::Transport, Connected), Error = Self::Error> + Send>;

    fn connect(&self, dst: Destination) -> Self::Future {
        let port = dst.port().unwrap_or(80);
        let conn = Socks5Stream::connect(self.proxy, (dst.host().to_owned(), port));
        Box::new(
            conn.into_future()
                .flatten()
                .map(|tcp| (tcp.into_inner(), Connected::new()))
                .map_err(|e| e.compat()),
        )
    }
}

fn main() {
    let connector = Connector {
        proxy: SocketAddr::from(([127, 0, 0, 1], 1086)),
    };
    let client = Client::builder().build::<_, hyper::Body>(connector);
    let future = client
        .get(Uri::from_static("http://httpbin.org/ip"))
        .and_then(|res| {
            println!("Response: {}", res.status());
            res.into_body()
                .for_each(|chunk| {
                    stdout()
                        .write_all(&chunk)
                        .map_err(|e| panic!("example expects stdout is open, error={}", e))
                })
        })
        .map_err(|err| {
            println!("Error: {}", err);
        });
    hyper::rt::run(future);
}
