//! Data-symbol cells for python310.dll — the LNK1194 workaround.
//!
//! `/DELAYLOAD:python310.dll` hides the python import from Pin's tool
//! loader, but the linker refuses delay-load for DATA symbols. pyo3-ffi
//! references twelve CPython data objects (exception singletons, the bool
//! type, None/True/False) as dllimport, i.e. through `__imp_<Name>` pointer
//! cells that normally live in the import address table. We define those
//! cells ourselves (plain pointer globals named exactly `__imp_<Name>`), so
//! the link needs NO data import from python310.dll and the delay-load
//! stays legal; the scripting thread fills every cell with GetProcAddress
//! right after preloading the DLL, before the first pyo3 call.
//!
//! The list below covers every `__imp_` data external in the pyo3/pyo3-ffi
//! rlibs plus the ones the agent's own code units reference (cross-checked
//! against python310.lib's import records: Type=data). Unused cells cost
//! one pointer and one GetProcAddress each. If a pyo3 upgrade pulls in more
//! data symbols, the link fails with LNK1194 naming them — add them here.

use core::ffi::{c_char, c_void};

extern "system" {
    fn GetProcAddress(module: *mut c_void, name: *const c_char) -> *mut c_void;
}

macro_rules! data_cells {
    ($($cell:ident [$x86_link_name:literal] => $export:literal),* $(,)?) => {
        $(
            // MSVC x86 decorates C symbols with one leading underscore.
            // PyO3's DATA reference is therefore __imp__<export>, whereas
            // x64 uses __imp_<export>. export_name supplies the pre-decoration
            // x86 spelling so the final COFF symbol matches PyO3 exactly.
            #[cfg_attr(target_arch = "x86", export_name = $x86_link_name)]
            #[cfg_attr(not(target_arch = "x86"), no_mangle)]
            #[allow(non_upper_case_globals)]
            pub static mut $cell: *mut c_void = core::ptr::null_mut();
        )*
        /// Fills every cell from the already-loaded python310.dll. Fails on
        /// the first unresolvable export (scripting then stays disabled).
        pub fn init_cells(module: *mut c_void) -> Result<(), &'static str> {
            unsafe {
                $(
                    let name = concat!($export, "\0").as_ptr() as *const c_char;
                    let address = GetProcAddress(module, name);
                    if address.is_null() {
                        return Err($export);
                    }
                    $cell = address;
                )*
            }
            Ok(())
        }
    };
}

data_cells! {
    __imp_PyBaseObject_Type ["_imp__PyBaseObject_Type"] => "PyBaseObject_Type",
    __imp_PyBool_Type ["_imp__PyBool_Type"] => "PyBool_Type",
    __imp_PyByteArray_Type ["_imp__PyByteArray_Type"] => "PyByteArray_Type",
    __imp_PyBytes_Type ["_imp__PyBytes_Type"] => "PyBytes_Type",
    __imp_PyCapsule_Type ["_imp__PyCapsule_Type"] => "PyCapsule_Type",
    __imp_PyComplex_Type ["_imp__PyComplex_Type"] => "PyComplex_Type",
    __imp_PyDict_Type ["_imp__PyDict_Type"] => "PyDict_Type",
    __imp_PyExc_AttributeError ["_imp__PyExc_AttributeError"] => "PyExc_AttributeError",
    __imp_PyExc_BaseException ["_imp__PyExc_BaseException"] => "PyExc_BaseException",
    __imp_PyExc_BlockingIOError ["_imp__PyExc_BlockingIOError"] => "PyExc_BlockingIOError",
    __imp_PyExc_BrokenPipeError ["_imp__PyExc_BrokenPipeError"] => "PyExc_BrokenPipeError",
    __imp_PyExc_ConnectionAbortedError ["_imp__PyExc_ConnectionAbortedError"] => "PyExc_ConnectionAbortedError",
    __imp_PyExc_ConnectionRefusedError ["_imp__PyExc_ConnectionRefusedError"] => "PyExc_ConnectionRefusedError",
    __imp_PyExc_ConnectionResetError ["_imp__PyExc_ConnectionResetError"] => "PyExc_ConnectionResetError",
    __imp_PyExc_FileExistsError ["_imp__PyExc_FileExistsError"] => "PyExc_FileExistsError",
    __imp_PyExc_FileNotFoundError ["_imp__PyExc_FileNotFoundError"] => "PyExc_FileNotFoundError",
    __imp_PyExc_ImportError ["_imp__PyExc_ImportError"] => "PyExc_ImportError",
    __imp_PyExc_InterruptedError ["_imp__PyExc_InterruptedError"] => "PyExc_InterruptedError",
    __imp_PyExc_OSError ["_imp__PyExc_OSError"] => "PyExc_OSError",
    __imp_PyExc_OverflowError ["_imp__PyExc_OverflowError"] => "PyExc_OverflowError",
    __imp_PyExc_PermissionError ["_imp__PyExc_PermissionError"] => "PyExc_PermissionError",
    __imp_PyExc_RuntimeError ["_imp__PyExc_RuntimeError"] => "PyExc_RuntimeError",
    __imp_PyExc_SystemError ["_imp__PyExc_SystemError"] => "PyExc_SystemError",
    __imp_PyExc_TimeoutError ["_imp__PyExc_TimeoutError"] => "PyExc_TimeoutError",
    __imp_PyExc_TypeError ["_imp__PyExc_TypeError"] => "PyExc_TypeError",
    __imp_PyExc_UnicodeDecodeError ["_imp__PyExc_UnicodeDecodeError"] => "PyExc_UnicodeDecodeError",
    __imp_PyExc_ValueError ["_imp__PyExc_ValueError"] => "PyExc_ValueError",
    __imp_PyFloat_Type ["_imp__PyFloat_Type"] => "PyFloat_Type",
    __imp_PyList_Type ["_imp__PyList_Type"] => "PyList_Type",
    __imp_PyLong_Type ["_imp__PyLong_Type"] => "PyLong_Type",
    __imp_PyModule_Type ["_imp__PyModule_Type"] => "PyModule_Type",
    __imp_PySlice_Type ["_imp__PySlice_Type"] => "PySlice_Type",
    __imp_PySuper_Type ["_imp__PySuper_Type"] => "PySuper_Type",
    __imp_PyTuple_Type ["_imp__PyTuple_Type"] => "PyTuple_Type",
    __imp_PyType_Type ["_imp__PyType_Type"] => "PyType_Type",
    __imp_PyUnicode_Type ["_imp__PyUnicode_Type"] => "PyUnicode_Type",
    __imp__PyWeakref_CallableProxyType ["_imp___PyWeakref_CallableProxyType"] => "_PyWeakref_CallableProxyType",
    __imp__PyWeakref_ProxyType ["_imp___PyWeakref_ProxyType"] => "_PyWeakref_ProxyType",
    __imp__PyWeakref_RefType ["_imp___PyWeakref_RefType"] => "_PyWeakref_RefType",
    __imp__Py_NoneStruct ["_imp___Py_NoneStruct"] => "_Py_NoneStruct",
    __imp__Py_NotImplementedStruct ["_imp___Py_NotImplementedStruct"] => "_Py_NotImplementedStruct",
    __imp__Py_EllipsisObject ["_imp___Py_EllipsisObject"] => "_Py_EllipsisObject",
    __imp__Py_TrueStruct ["_imp___Py_TrueStruct"] => "_Py_TrueStruct",
    __imp__Py_FalseStruct ["_imp___Py_FalseStruct"] => "_Py_FalseStruct",
}
