//! This module provides an idiomatic Rust API for creating and working with TVM functions.
//!
//! For calling an already registered TVM function use [`function::Builder`]
//! To register a TVM packed function from Rust side either
//! use [`function::register`] or the macro [`register_global_func`].
//!
//! See the tests and examples repository for more examples.

use std::{
    ffi::{CStr, CString},
    mem,
    os::raw::{c_char, c_int, c_void},
    ptr, slice, str,
    sync::Mutex,
};

use ts;

use ty::TypeCode;
use value::{TVMValue, ValueKind};
use ErrorKind;
use Module;
use Result;
use TVMArgValue;
use TVMRetValue;

lazy_static! {
    static ref GLOBAL_FUNCTION_NAMES: Mutex<Vec<&'static str>> = {
        let mut out_size = 0 as c_int;
        let name = ptr::null_mut() as *mut c_char;
        let mut out_array = name as *mut _;
        check_call!(ts::TVMFuncListGlobalNames(
            &mut out_size as *mut _,
            &mut out_array
        ));
        let names_list = unsafe { slice::from_raw_parts(out_array, out_size as usize) };
        Mutex::new(
            names_list
                .into_iter()
                .map(|&p| unsafe { CStr::from_ptr(p).to_str().unwrap() })
                .collect(),
        )
    };
}

/// Returns a registered TVM function by name.
pub fn get_global_func(name: &str, is_global: bool) -> Option<Function> {
    let name = CString::new(name).expect("function name should not contain any `0` byte");
    let mut handle = ptr::null_mut() as ts::TVMFunctionHandle;
    check_call!(ts::TVMFuncGetGlobal(
        name.as_ptr() as *const c_char,
        &mut handle as *mut _
    ));
    if !(handle.is_null()) {
        mem::forget(name);
        return Some(Function::new(handle, is_global, false));
    } else {
        None
    }
}

/// Wrapper around TVM function handle which includes `is_global`
/// indicating whether the function is global or not, `is_released`
/// to hint dropping the function handle and `is_cloned` showing
/// not to drop a cloned function from Rust side.
/// The value of these fields can be accessed through their respective methods.
#[derive(Debug, Hash)]
pub struct Function {
    pub(crate) handle: ts::TVMFunctionHandle,
    // whether the registered function is global or not.
    is_global: bool,
    // whether the function has been dropped from frontend or not.
    is_released: bool,
    // whether the function has been cloned from frontend or not.
    is_cloned: bool,
}

impl Function {
    pub(crate) fn new(handle: ts::TVMFunctionHandle, is_global: bool, is_released: bool) -> Self {
        Function {
            handle: handle,
            is_global: is_global,
            is_released: is_released,
            is_cloned: false,
        }
    }

    /// For a given function, it returns a function by name.
    pub fn get_function(name: &str, is_global: bool) -> Option<Function> {
        let gnames = GLOBAL_FUNCTION_NAMES.lock().unwrap();
        let fn_name = gnames.iter().find(|&&s| s == name)?;
        get_global_func(fn_name, is_global)
    }

    /// Returns the underlying TVM function handle.
    pub fn handle(&self) -> ts::TVMFunctionHandle {
        self.handle
    }

    /// Returns `true` if the underlying TVM function is global and `false` otherwise.
    pub fn is_global(&self) -> bool {
        self.is_global
    }

    /// Returns `true` if the underlying TVM function has been released
    /// from the frontend and `false` otherwise.
    pub fn is_released(&self) -> bool {
        self.is_released
    }

    /// Returns `true` if the underlying TVM function has been cloned
    /// from the frontend and `false` otherwise.
    pub fn is_cloned(&self) -> bool {
        self.is_cloned
    }
}

impl Clone for Function {
    fn clone(&self) -> Function {
        if !self.is_released && !self.is_cloned {
            Self {
                handle: self.handle,
                is_global: self.is_global,
                is_released: self.is_released,
                is_cloned: true,
            }
        } else {
            Function::new(self.handle, self.is_global, self.is_released)
        }
    }
}

impl Drop for Function {
    fn drop(&mut self) {
        if !self.is_released && !self.is_global && !self.is_cloned {
            check_call!(ts::TVMFuncFree(self.handle));
            self.is_released = true;
        }
    }
}

/// Function builder in order to create and call functions.
///
/// *Note:* Currently TVM functions accept *at most* one return value.
#[derive(Debug, Clone, Default)]
pub struct Builder<'a> {
    pub func: Option<Function>,
    pub arg_buf: Option<Box<[TVMArgValue<'a>]>>,
    pub ret_buf: Option<Box<[TVMRetValue]>>,
}

impl<'a> Builder<'a> {
    pub fn new(
        func: Option<Function>,
        arg_buf: Option<Box<[TVMArgValue<'a>]>>,
        ret_buf: Option<Box<[TVMRetValue]>>,
    ) -> Self {
        Self {
            func,
            arg_buf,
            ret_buf,
        }
    }

    pub fn get_function(&mut self, name: &str, is_global: bool) -> &mut Self {
        self.func = Function::get_function(name, is_global);
        self
    }

    /// Pushes a [`TVMArgValue`] into the function argument buffer.
    pub fn arg<'b, T: ?Sized>(&mut self, arg: &'b T) -> &mut Self
    where
        TVMValue: From<&'b T>,
        TypeCode: From<&'b T>,
    {
        let tvm_arg = TVMArgValue::from(arg);
        if self.arg_buf.is_none() {
            self.arg_buf = Some(Box::new([tvm_arg]));
        } else {
            let new_arg_buf = self.arg_buf.take().map(|bbuf| {
                let mut new_arg_buf = Vec::from(bbuf);
                new_arg_buf.push(tvm_arg);
                let new_len = new_arg_buf.len();
                new_arg_buf.truncate(new_len);
                new_arg_buf.into_boxed_slice()
            });
            self.arg_buf = new_arg_buf;
        }
        self
    }

    /// Pushes multiple [`TVMArgValue`]s into the function argument buffer.
    pub fn args<'b, T: 'b + ?Sized, I>(&mut self, args: I) -> &mut Self
    where
        I: IntoIterator<Item = &'b T>,
        TVMValue: From<&'b T>,
        TypeCode: From<&'b T>,
    {
        for arg in args {
            self.arg(&arg);
        }
        self
    }

    /// Sets an output for a function that requirs a mutable output to be provided.
    /// See the `basics` in tests for an example.
    pub fn set_output<'b, T: 'b + ?Sized>(&mut self, arg: &'b mut T) -> &mut Self
    where
        TVMValue: From<&'b T>,
        TypeCode: From<&'b T>,
    {
        let tvm_ret = TVMRetValue::new(TVMValue::from(arg), TypeCode::from(arg));
        if self.ret_buf.is_none() {
            self.ret_buf = Some(Box::new([tvm_ret]));
        } else {
            let new_ret_buf = self.ret_buf.take().map(|_| {
                let mut new_buf = Vec::with_capacity(1);
                new_buf.push(tvm_ret);
                new_buf.into_boxed_slice()
            });
            self.ret_buf = new_ret_buf;
        }
        self
    }

    /// Calls the function that created from `Builder`.
    pub fn invoke(&mut self) -> Result<TVMRetValue> {
        self.clone()(())
    }
}

impl<'a> FnOnce<((),)> for Builder<'a> {
    type Output = Result<TVMRetValue>;
    extern "rust-call" fn call_once(self, _: ((),)) -> Self::Output {
        if self.func.is_none() {
            bail!("{}", ErrorKind::FunctionNotFound);
        }
        let mut ret_val = unsafe { mem::uninitialized::<ts::TVMValue>() };
        let mut ret_type_code = 0 as c_int;
        if self.arg_buf.is_some() {
            let arg_buf = self.arg_buf?;
            let mut num_args = arg_buf.len();
            let mut values = arg_buf
                .iter()
                .map(|tav| tav.value.inner)
                .collect::<Vec<ts::TVMValue>>();
            let mut tcodes = arg_buf
                .iter()
                .map(|tav| tav.type_code as c_int)
                .collect::<Vec<_>>();
            if self.ret_buf.is_some() {
                num_args = num_args + 1;
                ret_val = *self.ret_buf.clone()?[0].value;
                ret_type_code = self.ret_buf.clone()?[0].type_code as c_int;
                values.append(&mut vec![ret_val]);
                tcodes.append(&mut vec![ret_type_code]);
            }
            values.truncate(num_args);
            tcodes.truncate(num_args);
            check_call!(ts::TVMFuncCall(
                self.func?.handle,
                values.as_mut_ptr(),
                tcodes.as_mut_ptr(),
                num_args as c_int,
                &mut ret_val as *mut _,
                &mut ret_type_code as *mut _
            ));
        } else {
            check_call!(ts::TVMFuncCall(
                self.func?.handle,
                ptr::null_mut(),
                ptr::null_mut(),
                0 as c_int,
                &mut ret_val as *mut _,
                &mut ret_type_code as *mut _
            ));
        }
        let ret = TVMRetValue::new(
            TVMValue::new(ValueKind::Return, ret_val),
            ret_type_code.into(),
        );
        Ok(ret)
    }
}

/// Converts a [`Function`] to builder. Currently, this is the best way to work with
/// TVM functions.
impl<'a> From<Function> for Builder<'a> {
    fn from(func: Function) -> Self {
        Builder::new(Some(func), None, None)
    }
}

/// Converts a mutable reference of a [`Module`] to [`Builder`].
impl<'a: 'b, 'b> From<&'b mut Module> for Builder<'a> {
    fn from(module: &mut Module) -> Self {
        Builder::new(module.entry.take(), None, None)
    }
}

unsafe extern "C" fn tvm_callback(
    args: *mut ts::TVMValue,
    type_codes: *mut c_int,
    num_args: c_int,
    ret: ts::TVMRetValueHandle,
    fhandle: *mut c_void,
) -> c_int {
    let len = num_args as usize;
    let args_list = slice::from_raw_parts_mut(args, len);
    let type_codes_list = slice::from_raw_parts_mut(type_codes, len);
    let mut local_args: Vec<TVMArgValue> = Vec::new();
    // due to unsafe mem::uninitialized rustc warning about unused `value` and `tcode`.
    let mut _value = mem::uninitialized::<ts::TVMValue>();
    let mut _tcode = mem::uninitialized::<c_int>();
    let rust_fn = mem::transmute::<*mut c_void, fn(&[TVMArgValue]) -> Result<TVMRetValue>>(fhandle);
    for i in 0..len {
        _value = args_list[i];
        _tcode = type_codes_list[i];
        if _tcode == TypeCode::kNodeHandle as c_int
            || _tcode == TypeCode::kFuncHandle as c_int
            || _tcode == TypeCode::kModuleHandle as c_int
        {
            check_call!(ts::TVMCbArgToReturn(&mut _value as *mut _, _tcode));
        }
        local_args.push(TVMArgValue::new(
            TVMValue::new(ValueKind::Handle, _value),
            _tcode.into(),
        ));
    }

    let rv = match rust_fn(local_args.as_slice()) {
        Ok(v) => v,
        Err(msg) => {
            ::set_last_error(&msg);
            return -1;
        }
    };
    let mut ret_val = *rv.value;
    let mut ret_type_code = rv.type_code as c_int;
    check_call!(ts::TVMCFuncSetReturn(
        ret,
        &mut ret_val as *mut _,
        &mut ret_type_code as *mut _,
        1 as c_int
    ));
    0
}

unsafe extern "C" fn tvm_callback_finalizer(fhandle: *mut c_void) {
    let rust_fn = mem::transmute::<*mut c_void, fn(&[TVMArgValue]) -> Result<TVMRetValue>>(fhandle);
    mem::drop(rust_fn);
}

fn convert_to_tvm_func(f: fn(&[TVMArgValue]) -> Result<TVMRetValue>) -> Function {
    let mut fhandle = ptr::null_mut() as ts::TVMFunctionHandle;
    let resource_handle = f as *mut fn(&[TVMArgValue]) -> Result<TVMRetValue>;
    check_call!(ts::TVMFuncCreateFromCFunc(
        Some(tvm_callback),
        resource_handle as *mut c_void,
        Some(tvm_callback_finalizer),
        &mut fhandle as *mut _
    ));
    Function::new(fhandle, false, false)
}

/// Registers a Rust function with signature
/// `fn(&[TVMArgValue]) -> Result<TVMRetValue>`
/// as a **global TVM packed function** from frontend to TVM backend.
///
/// Use [`register_global_func`] if overriding an existing global TVM function
/// is not required.
///
/// ## Example
///
/// ```
/// fn sum(args: &[TVMArgValue]) -> Result<TVMRetValue> {
///     let mut ret = 0;
///     for arg in args.iter() {
///         ret += arg.to_int();
///     }
///     let ret_val = TVMRetValue::from(&ret);
///     Ok(ret_val)
/// }
///
/// tvm::function::register(sum, "mysum".to_owned(), false).unwrap();
/// let mut registered = function::Builder::default();
/// registered.get_function("mysum", true);
/// assert!(registered.func.is_some());
/// registered.args(&[10, 20, 30]);
/// assert_eq!(registered.invoke().unwrap().to_int(), 60);
/// ```
pub fn register(
    f: fn(&[TVMArgValue]) -> Result<TVMRetValue>,
    name: String,
    override_: bool,
) -> Result<()> {
    let func = convert_to_tvm_func(f);
    let name = CString::new(name)?;
    check_call!(ts::TVMFuncRegisterGlobal(
        name.as_ptr() as *const c_char,
        func.handle(),
        override_ as c_int
    ));
    mem::forget(name);
    Ok(())
}

/// Convenient macro for registering functions from frontend to backend as global
/// TVM packed functions without overriding. If overriding an existing function is needed
/// use the [`function::register`] function instead.
///
/// ## Example
///
/// ```
/// register_global_func! {
///     fn sum(args: &[TVMArgValue]) -> Result<TVMRetValue> {
///         let mut ret = 0f64;
///         for arg in args.iter() {
///             ret += arg.to_float();
///         }
///         let ret_val = TVMRetValue::from(&ret);
///         Ok(ret_val)
///     }
/// }
///
/// let mut registered = function::Builder::default();
/// registered.get_function("sum", true);
/// assert!(registered.func.is_some());
/// registered.args(&[10f64, 20f64, 30f64]);
/// assert_eq!(registered.invoke().unwrap().to_float(), 60f64);
/// ```
#[macro_export]
macro_rules! register_global_func {
    {
        $(#[$m:meta])*
        fn $fn_name:ident($args:ident : &[TVMArgValue]) -> Result<TVMRetValue> {
            $($code:tt)*
        }
    } => {{
        $(#[$m])*
        fn $fn_name($args: &[TVMArgValue]) -> Result<TVMRetValue> {
            $($code)*
        }

        $crate::function::register($fn_name, stringify!($fn_name).to_owned(), false).unwrap();
    }}
}

/// Convenient macro for calling TVM packed functions by providing a
/// function identifier and some arguments. This macro outputs a `Result` type
/// and let user to perform proper error handling.
///
/// **Note**: this macro does *not* expect an outside mutable output. To
/// set mutable output use [`set_output`] directly in the builder pattern.
///
/// [`set_output`]:function/struct.Builder.html#method.set_output
///
/// ## Example
///
/// Instead of
///
/// ```
/// function::Builder::from(func).arg(&a).arg(&b).invoke();
/// ```
///
/// one can use
///
/// ```
/// call_packed!(func, &a, &b);
/// ```
#[macro_export]
macro_rules! call_packed {
    ($fn_name:ident, $($arg:expr),*) => {{
        let mut builder = $crate::function::Builder::from($fn_name);
        $(
            builder.arg($arg);
        )*
        builder.invoke()
    }}
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn list_global_func() {
        assert!(
            GLOBAL_FUNCTION_NAMES
                .lock()
                .unwrap()
                .iter()
                .find(|ref s| ***s == "tvm.graph_runtime.create")
                .is_some()
        );
    }

    #[test]
    fn get_fn() {
        assert!(Function::get_function("tvm.graph_runtime.remote_create", true).is_some());
        assert!(Function::get_function("does not exists!", false).is_none());
    }

    #[test]
    fn provide_args() {
        let mut func = Builder::default();
        func.get_function("tvm.graph_runtime.remote_create", true)
            .args(&[10, 20])
            .arg(&"test".to_owned());
        assert!(func.arg_buf.is_some());
        assert_eq!(func.arg_buf.take().map(|bv| Vec::from(bv).len()), Some(3));
    }
}
