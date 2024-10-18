#![allow(unused_imports)]
use std::{ffi::CString, mem::MaybeUninit, ptr::null};

use ucx1_sys::*;
use ucx_rpc::Error;
use ucx_rpc::ucp::*;

fn main() {
    let config = Config::default();
    config.print_to_stderr();

    let ctx = Context::new().unwrap();
    let worker = ctx.create_worker().unwrap();
    // worker.print_to_stderr();
    worker.progress();

    // let mut context = MaybeUninit::uninit();
    // let status = unsafe { ucp_init(&params, null(), context.as_mut_ptr()) };
}

// extern "C" {
//     static stderr: *mut FILE;
// }