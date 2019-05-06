use once_cell::sync::OnceCell;
use std::net::{SocketAddr, TcpStream as StdTcpStream};
use std::sync::Mutex;
use tokio::{
    io::{copy, read_exact, write_all},
    net::TcpListener,
    prelude::*,
    runtime::Runtime,
};
use tokio_socks::{
    tcp::{BindFuture, Socks5Stream},
    Error,
};

type Result<T> = std::result::Result<T, Error>;

pub const PROXY_ADDR: &'static str = "127.0.0.1:41080";
pub const ECHO_SERVER_ADDR: &'static str = "localhost:10007";
pub const MSG: &[u8] = b"hello";

pub fn echo_server(runtime: &mut Runtime) -> Result<()> {
    let listener = TcpListener::bind(&SocketAddr::from(([0, 0, 0, 0], 10007)))?;
    let fut = listener
        .incoming()
        .for_each(|tcp| {
            let (reader, writer) = tcp.split();
            tokio::spawn(copy(reader, writer).map(|_| ()).map_err(|_| ()));
            Ok(())
        })
        .map_err(|_| ());
    runtime.spawn(fut);
    Ok(())
}

pub fn test_connect<F>(conn: F) -> Result<()>
where
    F: Future<Item = Socks5Stream, Error = Error> + Send + 'static,
{
    let fut = conn
        .and_then(|tcp| write_all(tcp, MSG).map_err(Into::into))
        .and_then(|(tcp, _)| read_exact(tcp, [0; 5]).map_err(Into::into))
        .map(|(_, v)| v);
    let runtime = runtime();
    let res = runtime.lock().unwrap().block_on(fut)?;
    assert_eq!(&res[..], MSG);
    Ok(())
}

#[allow(dead_code)]
pub fn test_bind<S>(bind: BindFuture<'static, 'static, S>) -> Result<()>
where
    S: Stream<Item = SocketAddr, Error = Error> + Send + 'static,
{
    let fut = bind.and_then(|bind| {
        let bind_addr = bind.bind_addr().to_owned();
        tokio::spawn(
            bind.accept()
                .and_then(|tcp| {
                    let (reader, writer) = tcp.split();
                    copy(reader, writer).map(|_| ()).map_err(Into::into)
                })
                .map_err(|_| ()),
        );
        Ok(bind_addr)
    });
    let runtime = runtime();
    let bind_addr = runtime.lock().unwrap().block_on(fut)?;
    let mut tcp = StdTcpStream::connect(bind_addr)?;
    tcp.write_all(MSG)?;
    let mut buf = [0; 5];
    tcp.read_exact(&mut buf[..])?;
    assert_eq!(&buf[..], MSG);
    Ok(())
}

pub fn runtime() -> &'static Mutex<Runtime> {
    static RUNTIME: OnceCell<Mutex<Runtime>> = OnceCell::INIT;
    RUNTIME.get_or_init(|| {
        let mut runtime = Runtime::new().expect("Unable to create runtime");
        echo_server(&mut runtime).expect("Unable to bind");
        Mutex::new(runtime)
    })
}
