use super::*;
use crate::ucp::listener::ConnectionRequest;
use std::{cell::RefCell, net::SocketAddr, rc::Weak};
use socket2::SockAddr;
use tracing::error;

// 基本はRcで保持する
// err_handlerが呼ばれると自動的にcloseされたとみなし、Weakを通してclosedフラグを立てる
// closedフラグが立っていない場合はdropでucp_ep_close_nbxする
// ucp_ep_destroyはdropで呼ぶ
pub struct Endpoint {
  pub ptr: ucp_ep_h,
  pub closed: Rc<RefCell<bool>>,
  pub worker: Rc<Worker>,
}

pub struct StatusPtr {
  pub ptr: ucs_status_ptr_t,
}

impl StatusPtr {
  unsafe fn wait(self, worker: &Worker) -> Result<(), Error> {
      if !self.ptr.is_null() && UCS_PTR_STATUS(self.ptr) != ucs_status_t::UCS_OK {
          let mut checked_status = ucs_status_t::UCS_INPROGRESS;
          while checked_status == ucs_status_t::UCS_INPROGRESS {
              checked_status = ucp_request_check_status(self.ptr);
              worker.progress();
          }
          Error::from_status(checked_status)
      } else {
          Ok(())
      }
  }
}
impl Drop for StatusPtr {
  fn drop(&mut self) {
      if UCS_PTR_IS_PTR(self.ptr) {
          unsafe { ucp_request_free(self.ptr) }
      }
  }
}

impl Endpoint {
  pub unsafe fn from_sockaddr(worker: Rc<Worker>, addr: SocketAddr) -> Result<Self, Error> {
      unsafe extern "C" fn err_handler(user_data: *mut c_void, _: ucp_ep_h, _: ucs_status_t) {
          let closed_flag: Weak<RefCell<bool>> = Weak::from_raw(user_data as _);
          if let Some(closed_flag) = closed_flag.upgrade() {
              *closed_flag.borrow_mut() = true;
          }
      }
      let ep_params_default = MaybeUninit::uninit();
      let closed_flag = Rc::new(RefCell::new(false));
      let sockaddr = SockAddr::from(addr);
      let ep_params = ucp_ep_params {
          field_mask: (ucp_ep_params_field::UCP_EP_PARAM_FIELD_SOCK_ADDR
              | ucp_ep_params_field::UCP_EP_PARAM_FIELD_FLAGS
              | ucp_ep_params_field::UCP_EP_PARAM_FIELD_ERR_HANDLING_MODE
              | ucp_ep_params_field::UCP_EP_PARAM_FIELD_ERR_HANDLER)
              .0 as u64,
          err_mode: ucx1_sys::ucp_err_handling_mode_t::UCP_ERR_HANDLING_MODE_PEER,
          flags: ucp_ep_params_flags_field::UCP_EP_PARAMS_FLAGS_CLIENT_SERVER.0,
          sockaddr: ucs_sock_addr {
              addrlen: sockaddr.len(),
              addr: sockaddr.as_ptr() as _,
              // as *const sockaddr_storage as _,
          },
          err_handler: ucp_err_handler {
              cb: Some(err_handler),
              arg: Rc::downgrade(&closed_flag).as_ptr() as _,
          },
          ..ep_params_default.assume_init()
      };
      let mut ep = MaybeUninit::uninit();
      let status = ucp_ep_create(worker.handle, &ep_params, ep.as_mut_ptr());
      Error::from_status(status)?;
      worker.progress();
      Ok(Self {
          ptr: ep.assume_init(),
          closed: closed_flag,
          worker,
      })
  }

  unsafe fn from_conn_req(
      worker: Rc<Worker>,
      conn_req: ConnectionRequest,
  ) -> Result<Self, Error> {
      unsafe extern "C" fn err_handler(user_data: *mut c_void, _: ucp_ep_h, _: ucs_status_t) {
          let closed_flag: Weak<RefCell<bool>> = Weak::from_raw(user_data as _);
          if let Some(closed_flag) = closed_flag.upgrade() {
              *closed_flag.borrow_mut() = true;
          }
      }
      let ep_params_default = MaybeUninit::uninit();
      let closed_flag = Rc::new(RefCell::new(false));
      let ep_params = ucp_ep_params {
          field_mask: (ucp_ep_params_field::UCP_EP_PARAM_FIELD_CONN_REQUEST
              | ucp_ep_params_field::UCP_EP_PARAM_FIELD_ERR_HANDLING_MODE
              | ucp_ep_params_field::UCP_EP_PARAM_FIELD_ERR_HANDLER)
              .0 as u64,
          err_mode: ucx1_sys::ucp_err_handling_mode_t::UCP_ERR_HANDLING_MODE_PEER,
          err_handler: ucp_err_handler {
              cb: Some(err_handler),
              arg: Rc::downgrade(&closed_flag).as_ptr() as _,
          },
          conn_request: conn_req.ptr,
          ..ep_params_default.assume_init()
      };
      let mut ep = MaybeUninit::uninit();
      let status = ucp_ep_create(worker.handle, &ep_params, ep.as_mut_ptr());
      Error::from_status(status)?;
      Ok(Self {
          ptr: ep.assume_init(),
          closed: closed_flag,
          worker,
      })
  }

  unsafe fn tag_send<B: AsRef<[u8]>, C: Fn(ucs_status_t)>(
      &self,
      tag: u64,
      buffer: B,
      callback: Weak<C>,
  ) -> StatusPtr {
      unsafe extern "C" fn cb<C: Fn(ucs_status_t)>(
          _: *mut c_void,
          status: ucs_status_t,
          user_data: *mut c_void,
      ) {
          let state: Weak<C> = Weak::from_raw(user_data as _);
          if let Some(callback) = state.upgrade() {
              (callback)(status)
          }
      }
      let params_default = MaybeUninit::uninit();
      let params = ucp_request_param_t {
          op_attr_mask: (ucp_op_attr_t::UCP_OP_ATTR_FIELD_CALLBACK as u32
              | ucp_op_attr_t::UCP_OP_ATTR_FIELD_USER_DATA as u32
              | ucp_op_attr_t::UCP_OP_ATTR_FIELD_DATATYPE as u32),
          cb: ucp_request_param_t__bindgen_ty_1 {
              send: Some(cb::<C>),
          },
          user_data: callback.as_ptr() as _,
          datatype: ucp_dt_make_contig(1),
          ..params_default.assume_init()
      };
      let ptr = ucp_tag_send_nbx(
          self.ptr,
          buffer.as_ref().as_ptr() as _,
          buffer.as_ref().len(),
          tag,
          &params,
      );
      StatusPtr { ptr }
  }

  unsafe fn tag_recv<C: Fn(ucs_status_t)>(
      &self,
      buffer: &mut [MaybeUninit<u8>],
      tag: u64,
      #[allow(unused)] tag_mask: u64,
      callback: Weak<C>,
  ) -> StatusPtr {
      unsafe extern "C" fn cb<C: Fn(ucs_status_t)>(
          _: *mut c_void,
          status: ucs_status_t,
          _: *const ucp_tag_recv_info,
          user_data: *mut c_void,
      ) {
          let callback: Weak<C> = Weak::from_raw(user_data as _);
          if let Some(callback) = callback.upgrade() {
              (callback)(status)
          }
      }
      let params_default = MaybeUninit::uninit();
      let params = ucp_request_param_t {
          op_attr_mask: (ucp_op_attr_t::UCP_OP_ATTR_FIELD_CALLBACK as u32
              | ucp_op_attr_t::UCP_OP_ATTR_FIELD_USER_DATA as u32
              | ucp_op_attr_t::UCP_OP_ATTR_FIELD_DATATYPE as u32),
          datatype: ucp_dt_make_contig(1),
          cb: ucp_request_param_t__bindgen_ty_1 {
              recv: Some(cb::<C>),
          },
          user_data: callback.as_ptr() as _,
          ..params_default.assume_init()
      };
      let ptr = ucp_tag_recv_nbx(
          self.worker.handle,
          buffer.as_mut_ptr() as _,
          buffer.as_ref().len(),
          tag,
          tag_mask,
          &params,
      );
      StatusPtr { ptr }
  }
}

impl Drop for Endpoint {
  fn drop(&mut self) {
      if !*self.closed.borrow() {
          unsafe {
              let req_params_default = MaybeUninit::uninit();
              let req_params = ucp_request_param_t {
                  op_attr_mask: ucp_op_attr_t::UCP_OP_ATTR_FIELD_FLAGS as u32,
                  flags: ucp_ep_close_flags_t::UCP_EP_CLOSE_FLAG_FORCE.0,
                  ..req_params_default.assume_init()
              };
              let status = ucp_ep_close_nbx(self.ptr, &req_params);
              let status = StatusPtr { ptr: status };
              if let Err(e) = status.wait(&self.worker) {
                  error!("{e}");
              }
          }
      }
      // unsafe { ucp_ep_destroy(self.ptr) }
  }
}