// Copyright 2018 Ehsan M. Kermani.
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// http://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or http://opensource.org/licenses/MIT>, at your
// option. This file may not be copied, modified, or distributed
// except according to those terms.
#![crate_name = "tvm_rust"]
#![doc(html_root_url = "https://docs.rs/tvm-rust/0.0.2/")]
#![allow(
    non_camel_case_types,
    unused_imports,
    dead_code,
    unused_variables
)]
#![feature(try_from, fn_traits, unboxed_closures)]

//! [WIP]
//!
//! The `tvm_rust` crate aims at supporting Rust as one of the frontend API in
//! [TVM](https://github.com/dmlc/tvm) runtime.
//!

extern crate ndarray as rust_ndarray;
extern crate ordered_float;
extern crate tvm_sys as tvm;

use std::convert::From;
use std::collections::HashMap;
use std::cell::RefCell;
use std::error::Error;
use std::ffi::{CStr, CString};
use std::fmt;
use std::fmt::{Debug, Display, Formatter};
use std::hash::{Hash, Hasher};
use std::os::raw::{c_char, c_int, c_void};
use std::ptr;
use std::result;
use std::str;

/// Macro to check the return call to TVM runtime shared library
macro_rules! check_call {
    ($e:expr) => {{
        if unsafe { $e } != 0 {
            panic!("{}", ::TVMError::get_last());
        }
    }};
}

// TODO: make it robust thread_local for ffi set
/// TVM error type
#[derive(Debug)]
pub struct TVMError;

impl TVMError {
    /// Get the last error message from TVM
    pub fn get_last() -> &'static str {
        unsafe {
            match CStr::from_ptr(tvm::TVMGetLastError()).to_str() {
                Ok(s) => s,
                Err(_) => "Invalid UTF-8 message",
            }
        }
    }

    pub fn set_last(msg: &'static str) {
        unsafe {
            tvm::TVMAPISetLastError(msg.as_ptr() as *const c_char);
        }
    }
}

impl Display for TVMError {
    fn fmt(&self, f: &mut Formatter) -> fmt::Result {
        write!(f, "{}", TVMError::get_last())
    }
}

impl Error for TVMError {
    fn description(&self) -> &'static str {
        TVMError::get_last()
    }

    fn cause(&self) -> Option<&Error> {
        None
    }
}

pub mod function;
pub mod module;
pub mod ndarray;
pub mod context;
pub mod value;
pub mod ty;

pub use function::Function;
pub use module::Module;
pub use ndarray::{empty, NDArray};
pub use context::*;
pub use value::*;
pub use ty::*;

//type f32 = tvm::f32;
//type f64 = tvm::f64;

/// TVM result type
pub type TVMResult<T> = result::Result<T, TVMError>;

pub fn version() -> &'static str {
    str::from_utf8(tvm::TVM_VERSION).unwrap()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn context() {
        let ctx = TVMContext::cpu(0);
        let default_ctx = TVMContext::new(TVMDeviceType::new(tvm::DLDeviceType::kDLCPU), 0);
        assert_eq!(ctx.current_context().clone(), default_ctx);
        assert_ne!(ctx, TVMContext::gpu(0));

        let str_ctx = TVMContext::new(TVMDeviceType::from("gpu"), 0);
        assert_eq!(str_ctx.current_context().clone(), str_ctx);
        assert_ne!(str_ctx, TVMContext::new(TVMDeviceType::from("cpu"), 0));
    }

    #[test]
    fn print_version() {
        println!("TVM version: {}", version());
    }

    #[test]
    fn sync() {
        let ctx = TVMContext::cpu(0);
        assert!(ctx.sync().is_ok())
    }

    #[test]
    fn error() {
        let msg: &'static str = "Invalid";
        TVMError::set_last(msg);
        assert_eq!(TVMError::get_last().trim(), msg);
    }
}
