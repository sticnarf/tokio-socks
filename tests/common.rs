use once_cell::sync::OnceCell;
use std::net::SocketAddr;
use std::sync::Mutex;
use tokio::{io, net::TcpListener, prelude::*, runtime::Runtime};
use tokio_socks::Error;

type Result<T> = std::result::Result<T, Error>;

pub fn echo_server(runtime: &mut Runtime) -> Result<()> {
    let listener = TcpListener::bind(&SocketAddr::from(([127, 0, 0, 1], 10007)))?;
    let fut = listener
        .incoming()
        .for_each(|tcp| {
            let (read, write) = tcp.split();
            tokio::spawn(io::copy(read, write).map(|_| ()).map_err(|_| ()));
            Ok(())
        })
        .map_err(|_| ());
    runtime.spawn(fut);
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
