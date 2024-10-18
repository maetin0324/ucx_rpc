use tracing::trace;

use crate::Error;

use super::*;
use std::rc::Weak;
// use futures::channel::mpsc;
// use futures::stream::StreamExt;
use std::sync::mpsc;
use std::mem::MaybeUninit;
use std::net::SocketAddr;

struct ConnHandlerData<S> {
  cb: unsafe fn(ConnectionRequest, Rc<Worker>, S),
  state: S,
  worker: Rc<Worker>,
}

struct Listener<S> {
  h: ucp_listener_h,
  _worker: Rc<Worker>,
  _conn_handler_data: Rc<ConnHandlerData<S>>,
}

impl<S> Drop for Listener<S> {
  fn drop(&mut self) {
      unsafe {
          ucp_listener_destroy(self.h);
      }
  }
}

impl<S: Clone> Listener<S> {
  unsafe fn create(
      worker: &Rc<Worker>,
      addr: SocketAddr,
      cb: unsafe fn(ConnectionRequest, Rc<Worker>, S),
      state: S,
  ) -> Result<Rc<Listener<S>>, Error> {
      let listener_params_default = MaybeUninit::uninit();
      let sockaddr = socket2::SockAddr::from(addr);
      let sockaddr = ucs_sock_addr {
          addrlen: sockaddr.len(),
          addr: (&sockaddr.as_storage() as *const libc::sockaddr_storage)
              as *const ucx1_sys::sockaddr,
      };
      unsafe extern "C" fn callback<S: Clone>(
          conn_request: *mut ucx1_sys::ucp_conn_request,
          user_data: *mut c_void,
      ) {
          let user_data: Weak<ConnHandlerData<S>> = Weak::from_raw(user_data as _);
          let conn_request = ConnectionRequest::from_raw(conn_request);
          if let Some(user_data) = user_data.upgrade() {
              (user_data.cb)(
                  conn_request,
                  user_data.worker.clone(),
                  user_data.state.clone(),
              );
          }
      }

      let conn_handler_data = Rc::new(ConnHandlerData {
          cb,
          state: state.clone(),
          worker: worker.clone(),
      });

      let conn_handler = ucx1_sys::ucp_listener_conn_handler {
          cb: Some(callback::<S>),
          arg: Rc::downgrade(&conn_handler_data).as_ptr() as _,
      };

      let listener_params = ucp_listener_params_t {
          field_mask: (ucp_listener_params_field::UCP_LISTENER_PARAM_FIELD_SOCK_ADDR
              | ucp_listener_params_field::UCP_LISTENER_PARAM_FIELD_CONN_HANDLER)
              .0 as u64,
          sockaddr,
          conn_handler,
          ..unsafe { listener_params_default.assume_init() }
      };

      let mut listener = MaybeUninit::uninit();

      let status = ucp_listener_create(worker.handle, &listener_params, listener.as_mut_ptr());
      Error::from_status(status)?;
      let listener = listener.assume_init();
      let listener = Listener {
          h: listener,
          _worker: worker.clone(),
          _conn_handler_data: conn_handler_data,
      };
      Ok(Rc::new(listener))
  }
}

pub struct ConnectionRequest {
  pub ptr: *mut ucp_conn_request,
}

impl ConnectionRequest {
  unsafe fn from_raw(ptr: *mut ucp_conn_request) -> Self {
      Self { ptr }
  }
}
