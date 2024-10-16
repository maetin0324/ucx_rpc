use std::{ffi::CString, mem::MaybeUninit, ptr::null};

use ucx1_sys::*;
use ucx_rpc::Error;
use ucx_rpc::ucp::*;

fn main() {
    let mut handle = MaybeUninit::uninit();
    let status = unsafe { ucp_config_read(null(), null(), handle.as_mut_ptr()) };
    Error::from_status(status).unwrap();
    let handle = unsafe { handle.assume_init() };
    let flags = ucs_config_print_flags_t::UCS_CONFIG_PRINT_CONFIG
            | ucs_config_print_flags_t::UCS_CONFIG_PRINT_DOC
            | ucs_config_print_flags_t::UCS_CONFIG_PRINT_HEADER
            | ucs_config_print_flags_t::UCS_CONFIG_PRINT_HIDDEN;
    let title = CString::new("UCP Configuration").expect("Not a valid CStr");
    unsafe { ucp_config_print(handle, stderr, title.as_ptr(), flags) };
    unsafe { ucp_config_release(handle) };

    let features = ucp_feature::UCP_FEATURE_RMA | ucp_feature::UCP_FEATURE_AM;

    let params = ucp_params_t {
        field_mask: (ucp_params_field::UCP_PARAM_FIELD_FEATURES
            | ucp_params_field::UCP_PARAM_FIELD_REQUEST_SIZE
            | ucp_params_field::UCP_PARAM_FIELD_REQUEST_INIT
            | ucp_params_field::UCP_PARAM_FIELD_REQUEST_CLEANUP
            | ucp_params_field::UCP_PARAM_FIELD_MT_WORKERS_SHARED)
            .0 as u64,
        features: features.0 as u64,
        request_size: 0,
        request_init: None,
        request_cleanup: None,
        mt_workers_shared: 1,
        ..unsafe { MaybeUninit::uninit().assume_init() }
    };

    let ctx = Context::new().unwrap();
    let worker = ctx.create_worker().unwrap();
    worker.print_to_stderr();
    worker.progress();

    // let mut context = MaybeUninit::uninit();
    // let status = unsafe { ucp_init(&params, null(), context.as_mut_ptr()) };
}

extern "C" {
    static stderr: *mut FILE;
}