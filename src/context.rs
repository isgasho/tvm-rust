//! Provides [`TVMContext`] and related device specific informations.
//!
//! Create a new context by device type (cpu is 1) and device id.
//!
//! # Example
//!
//! ```
//! let ctx = TVMContext::new(1, 0);
//! let cpu0 = TVMContext::cpu(0);
//! assert_eq!(ctx, cpu0);
//! ```
//! Or from supported device name.
//!
//! ```
//! let cpu0 = TVMContext::from("cpu");
//! println!("{}", cpu0);
//! ```

use std::{
    fmt::{self, Display, Formatter},
    os::raw::c_void,
    ptr,
};

use function;
use internal_api;
use ts;
use Result;

/// Device type which can be from a supported device name. See the supported devices
/// in [TVM](https://github.com/dmlc/tvm) project.
///
/// ## Example
///
/// ```
/// let cpu = TVMDeviceType::from("cpu");
/// println!("device is: {}", cpu);
///```

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct TVMDeviceType(pub usize);

impl Default for TVMDeviceType {
    /// default device is cpu.
    fn default() -> Self {
        TVMDeviceType(1)
    }
}

impl From<TVMDeviceType> for ts::DLDeviceType {
    fn from(device_type: TVMDeviceType) -> Self {
        match device_type.0 {
            1 => ts::DLDeviceType_kDLCPU,
            2 => ts::DLDeviceType_kDLGPU,
            3 => ts::DLDeviceType_kDLCPUPinned,
            4 => ts::DLDeviceType_kDLOpenCL,
            7 => ts::DLDeviceType_kDLVulkan,
            8 => ts::DLDeviceType_kDLMetal,
            9 => ts::DLDeviceType_kDLVPI,
            10 => ts::DLDeviceType_kDLROCM,
            12 => ts::DLDeviceType_kDLExtDev,
            _ => panic!("device type not found!"),
        }
    }
}

impl From<ts::DLDeviceType> for TVMDeviceType {
    fn from(device_type: ts::DLDeviceType) -> Self {
        match device_type {
            ts::DLDeviceType_kDLCPU => TVMDeviceType(1),
            ts::DLDeviceType_kDLGPU => TVMDeviceType(2),
            ts::DLDeviceType_kDLCPUPinned => TVMDeviceType(3),
            ts::DLDeviceType_kDLOpenCL => TVMDeviceType(4),
            ts::DLDeviceType_kDLVulkan => TVMDeviceType(7),
            ts::DLDeviceType_kDLMetal => TVMDeviceType(8),
            ts::DLDeviceType_kDLVPI => TVMDeviceType(9),
            ts::DLDeviceType_kDLROCM => TVMDeviceType(10),
            ts::DLDeviceType_kDLExtDev => TVMDeviceType(12),
            _ => panic!("device type not found!"),
        }
    }
}

impl Display for TVMDeviceType {
    fn fmt(&self, f: &mut Formatter) -> fmt::Result {
        write!(
            f,
            "{}",
            match self {
                TVMDeviceType(1) => "cpu",
                TVMDeviceType(2) => "gpu",
                TVMDeviceType(3) => "cpu_pinned",
                TVMDeviceType(4) => "opencl",
                TVMDeviceType(8) => "meta",
                TVMDeviceType(9) => "vpi",
                TVMDeviceType(10) => "rocm",
                TVMDeviceType(_) => "rpc",
            }
        )
    }
}

impl<'a> From<&'a str> for TVMDeviceType {
    fn from(type_str: &'a str) -> Self {
        match type_str {
            "cpu" => TVMDeviceType(1),
            "llvm" => TVMDeviceType(1),
            "stackvm" => TVMDeviceType(1),
            "gpu" => TVMDeviceType(2),
            "cuda" => TVMDeviceType(2),
            "nvptx" => TVMDeviceType(2),
            "cl" => TVMDeviceType(4),
            "opencl" => TVMDeviceType(4),
            "metal" => TVMDeviceType(8),
            "vpi" => TVMDeviceType(9),
            "rocm" => TVMDeviceType(10),
            _ => panic!("{:?} not supported!", type_str),
        }
    }
}

/// Represents the underlying device context. Default is cpu.
///
/// ## Examples
///
/// ```
/// let ctx = TVMContext::from("gpu");
/// assert!(ctx.exist());
///
/// ```
/// It is possible to query the context and get information such as
///
/// ```
/// println!("maximun threads per block: {}", ctx.max_threads_per_block());
/// println!("compute version: {}", ctx.compute_version());
/// ```

#[derive(Debug, Default, Clone, Hash, PartialEq, Eq)]
pub struct TVMContext {
    /// Supported device types
    pub device_type: TVMDeviceType,
    /// Device id
    pub device_id: usize,
}

impl TVMContext {
    /// Creates context from device type and id.
    pub fn new(device_type: TVMDeviceType, device_id: usize) -> Self {
        TVMContext {
            device_type: device_type,
            device_id: device_id,
        }
    }
}

macro_rules! impl_ctxs {
    ($(($ctx:ident, $dldevt:expr));+) => {
        $(
            impl TVMContext {
                pub fn $ctx(device_id: usize) -> Self {
                    Self::new(TVMDeviceType($dldevt), device_id)
                }
            }
        )+
    };
}

impl_ctxs!((cpu, 1);
            (gpu, 2);
            (nvptx, 2);
            (cuda, 2);
            (cpu_pinned, 3);
            (cl, 4);
            (opencl, 4);
            (metal, 8);
            (vpi, 9);
            (rocm, 10));

impl<'a> From<&'a str> for TVMContext {
    fn from(target: &str) -> Self {
        TVMContext::new(TVMDeviceType::from(target), 0)
    }
}

impl TVMContext {
    /// Checks whether the context exists or not.
    pub fn exist(&self) -> bool {
        let func = internal_api::get_api("_GetDeviceAttr".to_owned());
        let dt = self.device_type.0 as usize;
        // `unwrap` is ok here because if there is any error,
        // if would occure inside `call_packed!`
        let ret = call_packed!(func, &dt, &self.device_id, &0).unwrap();
        ret.to_int() != 0
    }

    /// Synchronize the context stream.
    pub fn sync(&self) -> Result<()> {
        check_call!(ts::TVMSynchronize(
            self.device_type.0 as i32,
            self.device_id as i32,
            ptr::null_mut() as *mut c_void
        ));
        Ok(())
    }
}

macro_rules! impl_dev_attrs {
    ($attr_name:ident, $attr_kind:expr) => {
        impl TVMContext {
            pub fn $attr_name(&self) -> usize {
                let func = ::internal_api::get_api("_GetDeviceAttr".to_owned());
                let dt = self.device_type.0 as usize;
                // `unwrap` is ok here because if there is any error,
                // if would occur in function call.
                let ret = function::Builder::from(func)
                    .args(&[dt, self.device_id, $attr_kind])
                    .invoke()
                    .unwrap();
                ret.to_int() as usize
            }
        }
    };
}

impl_dev_attrs!(max_threads_per_block, 1);
impl_dev_attrs!(warp_size, 2);
impl_dev_attrs!(max_shared_memory_per_block, 3);
impl_dev_attrs!(compute_version, 4);
impl_dev_attrs!(device_name, 5);
impl_dev_attrs!(max_clock_rate, 6);
impl_dev_attrs!(multi_processor_count, 7);
impl_dev_attrs!(max_thread_dimensions, 8);

impl From<ts::DLContext> for TVMContext {
    fn from(ctx: ts::DLContext) -> Self {
        TVMContext {
            device_type: TVMDeviceType::from(ctx.device_type),
            device_id: ctx.device_id as usize,
        }
    }
}

impl From<TVMContext> for ts::DLContext {
    fn from(ctx: TVMContext) -> Self {
        ts::DLContext {
            device_type: ctx.device_type.into(),
            device_id: ctx.device_id as i32,
        }
    }
}

impl Display for TVMContext {
    fn fmt(&self, f: &mut Formatter) -> fmt::Result {
        write!(f, "{}({})", self.device_type, self.device_id)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn context() {
        let ctx = TVMContext::cpu(0);
        println!("ctx: {}", ctx);
        let default_ctx = TVMContext::new(TVMDeviceType(1), 0);
        assert_eq!(ctx.clone(), default_ctx);
        assert_ne!(ctx, TVMContext::gpu(0));

        let str_ctx = TVMContext::new(TVMDeviceType::from("gpu"), 0);
        assert_eq!(str_ctx.clone(), str_ctx);
        assert_ne!(str_ctx, TVMContext::new(TVMDeviceType::from("cpu"), 0));
    }

    #[test]
    fn sync() {
        let ctx = TVMContext::cpu(0);
        assert!(ctx.sync().is_ok())
    }
}
