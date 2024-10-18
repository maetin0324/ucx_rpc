#![allow(unused_imports)]

use std::net::{SocketAddr, SocketAddrV4};
use std::rc::Rc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Instant;
use std::{ffi::CString, mem::MaybeUninit, ptr::null};

use endpoint::Endpoint;
use listener::{ConnectionRequest, Listener};
use tracing::{info, warn};
use tracing_subscriber::layer::SubscriberExt;
use tracing_subscriber::util::SubscriberInitExt;
use ucx_rpc::Error;
use ucx_rpc::ucp::*;

const MESSAGE: &str = "Hello, World!";

fn main() -> anyhow::Result<()> {
  tracing_subscriber::registry()
    .with(
      tracing_subscriber::EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| "Info".into())
    )
    .with(tracing_subscriber::fmt::Layer::default().with_ansi(true))
    .init();
  let port = 10301;
  if let Some(ip) = std::env::args().nth(1) {
    let ctx = Context::new().unwrap();
    unsafe {
      let worker = ctx.create_worker().unwrap();
      let ep = match Endpoint::from_sockaddr(
        worker,
        SocketAddr::V4(SocketAddrV4::new(ip.parse().unwrap(), port)),
      ) {
        Ok(ep) => ep,
        Err(e) => {
          warn!("from_sockaddr: {e}");
          return Err(e.into());
        }
      };
      if let Err(e) = client_server_do_work(ep, false) {
        warn!("client_server_do_work: {e}");
      }
      info!("Client done");
    }
  } else {
    let ctx = Context::new().unwrap();
    let worker = ctx.create_worker().unwrap();
    let end_frag = Rc::new(AtomicBool::new(false));
    unsafe {
      let listener = Listener::create(
        &worker,
        SocketAddr::V4(SocketAddrV4::new("0.0.0.0".parse().unwrap(), port)),
        conn_handler,
        end_frag.clone(),
      )?;
      info!("Server started on port {port}");
      info!("Server waiting on listener: {:?}", listener);
      while !end_frag.load(Ordering::Relaxed) {
        worker.progress();
      }
    }
    info!("Server done");
  }
  Ok(())
}


unsafe fn client_server_do_work(ep: Endpoint, is_server: bool) -> Result<(), Error> {
  info!("client_server_do_work ep: {:?}", ep);
  if is_server {
      let mut buffer = [MaybeUninit::uninit(); 256];
      let rx_cb = Rc::new(|_| {});
      // let tx_cb = Rc::new(|_| {});

      // for _ in 0..50 {
      //     for _ in 0..100_000 {
      //         let status = ep.tag_recv(&mut buffer, 99, 0, Rc::downgrade(&rx_cb));
      //         status.wait(&ep.worker)?;
      //         let buffer: [u8; 256] = std::mem::transmute(buffer);
      //         let status = ep.tag_send(101, buffer, Rc::downgrade(&tx_cb));
      //         status.wait(&ep.worker)?;
      //     }
      // }
      let status = ep.tag_recv(&mut buffer, 99, 0, Rc::downgrade(&rx_cb));
      info!("server_do_work tag_recv: {:?}", status.status());
      status.wait(&ep.worker)?;
      let buffer: [u8; 256] = std::mem::transmute(buffer);
      info!("received message: {:?}", std::str::from_utf8(&buffer).unwrap());
      info!("received all messages");

      Ok(())
  } else {
      // let rx_cb = Rc::new(|_| {});
      let tx_cb = Rc::new(|_| {});

      // for _ in 0..50 {
      //     let now = Instant::now();
      //     for _ in 0..100_000 {
      //         let status = ep.tag_send(99, MESSAGE.as_bytes(), Rc::downgrade(&tx_cb));
      //         info!("client_do_work tag_send: {:?}", status);
      //         status.wait(&ep.worker)?;

      //         let mut buffer = [MaybeUninit::uninit(); 256];
      //         let status = ep.tag_recv(&mut buffer, 101, 0, Rc::downgrade(&rx_cb));
      //         status.wait(&ep.worker)?;
      //     }
      //     let elapsed = now.elapsed().as_micros();
      //     info!(iops = (100000.0 / elapsed as f64 * 1000.0 * 1000.0))
      // }

      let status = ep.tag_send(99, MESSAGE.as_bytes(), Rc::downgrade(&tx_cb));
      info!("client_do_work tag_send: {:?}", status.status());
      // ep.print_to_stderr();
      status.wait(&ep.worker)?;

      Ok(())
  }
}

unsafe fn conn_handler(conn_req: ConnectionRequest, worker: Rc<Worker>, state: Rc<AtomicBool>) {
  info!("Connection request received");
  let ep = match Endpoint::from_conn_req(worker, conn_req) {
      Ok(ep) => ep,
      Err(e) => {
          warn!("{e}");
          return;
      }
  };
  info!("Connection established ep: {:?}", ep);
  if let Err(e) = client_server_do_work(ep, true) {
      warn!("{e}");
  }
  state.store(true, Ordering::SeqCst);
}


