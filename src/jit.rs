// ── JIT compilation via libtcc ──────────────────────────────────────────────
//
// Optional runtime loading of libtcc.so for in-process C JIT execution.
// Falls back gracefully if libtcc is not installed — `hut run --jit` will
// print a helpful message asking the user to install TCC.

use std::ffi::{c_char, c_int, c_void, CString};
use std::path::Path;

use anyhow::{bail, Context};

// ── TCC constants ──────────────────────────────────────────────────────────

const TCC_OUTPUT_MEMORY: c_int = 1;

// ── FFI type aliases ───────────────────────────────────────────────────────

type TCCState = c_void;

// Function pointer types
type TccNewFn = unsafe extern "C" fn() -> *mut TCCState;
type TccDeleteFn = unsafe extern "C" fn(*mut TCCState);
type TccSetOutputTypeFn = unsafe extern "C" fn(*mut TCCState, c_int) -> c_int;
type TccCompileStringFn = unsafe extern "C" fn(*mut TCCState, *const c_char) -> c_int;
type TccSetOptionsFn = unsafe extern "C" fn(*mut TCCState, *const c_char) -> c_int;
type TccRelocateFn = unsafe extern "C" fn(*mut TCCState, *const c_void) -> c_int;
type TccGetSymbolFn = unsafe extern "C" fn(*mut TCCState, *const c_char) -> *mut c_void;

// ── Dynamically loaded libtcc wrapper ──────────────────────────────────────

/// A wrapper around a dynamically-loaded libtcc instance.
/// The library is kept alive for the lifetime of this struct.
pub struct Tcc {
    _lib: libloading::Library,
    delete: TccDeleteFn,
    set_options: TccSetOptionsFn,
    compile_string: TccCompileStringFn,
    relocate: TccRelocateFn,
    get_symbol: TccGetSymbolFn,
    state: *mut TCCState,
}

// Safety: libtcc is thread-safe when each TCCState is used from one thread.
unsafe impl Send for Tcc {}

impl Tcc {
    /// Try to load libtcc.so from the system.
    /// Returns `None` if TCC is not installed.
    pub fn new() -> Option<Self> {
        // Search paths in order
        let candidates = [
            "/home/hermes/.local/lib/libtcc.so",
            "/usr/local/lib/libtcc.so",
            "/usr/lib/libtcc.so",
            "libtcc.so.0",
            "libtcc.so",
        ];

        let lib = candidates.iter().find_map(|p| {
            // Only try to load if the file exists for absolute paths
            if p.starts_with('/') && !Path::new(p).exists() {
                return None;
            }
            unsafe { libloading::Library::new(*p).ok() }
        })?;

        unsafe {
            // Get raw function pointers via transmute to avoid lifetime issues
            let new: TccNewFn = {
                let sym: libloading::Symbol<TccNewFn> = lib.get(b"tcc_new").ok()?;
                std::mem::transmute_copy(&sym)
            };
            let delete: TccDeleteFn = {
                let sym: libloading::Symbol<TccDeleteFn> = lib.get(b"tcc_delete").ok()?;
                std::mem::transmute_copy(&sym)
            };
            let set_output_type: TccSetOutputTypeFn = {
                let sym: libloading::Symbol<TccSetOutputTypeFn> =
                    lib.get(b"tcc_set_output_type").ok()?;
                std::mem::transmute_copy(&sym)
            };
            let compile_string: TccCompileStringFn = {
                let sym: libloading::Symbol<TccCompileStringFn> =
                    lib.get(b"tcc_compile_string").ok()?;
                std::mem::transmute_copy(&sym)
            };
            let set_options: TccSetOptionsFn = {
                let sym: libloading::Symbol<TccSetOptionsFn> =
                    lib.get(b"tcc_set_options").ok()?;
                std::mem::transmute_copy(&sym)
            };
            let relocate: TccRelocateFn = {
                let sym: libloading::Symbol<TccRelocateFn> = lib.get(b"tcc_relocate").ok()?;
                std::mem::transmute_copy(&sym)
            };
            let get_symbol: TccGetSymbolFn = {
                let sym: libloading::Symbol<TccGetSymbolFn> = lib.get(b"tcc_get_symbol").ok()?;
                std::mem::transmute_copy(&sym)
            };

            // Create the TCC state
            let state = (new)();
            if state.is_null() {
                return None;
            }

            // Set to memory output mode for JIT
            if (set_output_type)(state, TCC_OUTPUT_MEMORY) != 0 {
                (delete)(state);
                return None;
            }

            Some(Tcc {
                _lib: lib,
                delete,
                set_options,
                compile_string,
                relocate,
                get_symbol,
                state,
            })
        }
    }

    /// Set TCC options (e.g. "-g", "-O2", "-DNDEBUG").
    /// Must be called before compile().
    pub fn set_options(&mut self, options: &str) -> anyhow::Result<()> {
        let c_opts = CString::new(options).context("Options contained null bytes")?;
        let ret = unsafe { (self.set_options)(self.state, c_opts.as_ptr()) };
        if ret == -1 {
            bail!("TCC set_options failed: {options}");
        }
        Ok(())
    }

    /// Compile a C source string.
    pub fn compile(&mut self, source: &str) -> anyhow::Result<()> {
        let c_source = CString::new(source).context("Source contained null bytes")?;
        let ret = unsafe { (self.compile_string)(self.state, c_source.as_ptr()) };
        if ret == -1 {
            bail!("TCC compilation failed");
        }
        Ok(())
    }

    /// Relocate and finalize the compiled code.
    pub fn relocate(&mut self) -> anyhow::Result<()> {
        let ret = unsafe { (self.relocate)(self.state, std::ptr::null()) };
        if ret == -1 {
            bail!("TCC relocation failed");
        }
        Ok(())
    }

    /// Get a symbol address (e.g., `main` function pointer).
    pub fn get_symbol(&self, name: &str) -> Option<*mut c_void> {
        let c_name = CString::new(name).ok()?;
        let ptr = unsafe { (self.get_symbol)(self.state, c_name.as_ptr()) };
        if ptr.is_null() {
            None
        } else {
            Some(ptr)
        }
    }

    /// Run the compiled code by calling `main(argc, argv)`.
    pub fn run_main(&mut self, args: &[String]) -> anyhow::Result<i32> {
        let main_ptr = self
            .get_symbol("main")
            .context("No `main` symbol found in JIT-compiled code")?;

        type MainFn = unsafe extern "C" fn(c_int, *const *const c_char) -> c_int;
        let main: MainFn = unsafe { std::mem::transmute(main_ptr) };

        // Build C-compatible argv
        let c_args: Vec<CString> = args
            .iter()
            .map(|a| CString::new(a.as_str()).unwrap())
            .collect();
        let mut argv: Vec<*const c_char> = c_args.iter().map(|a| a.as_ptr()).collect();

        // Always set argv[0] to the program name or "hut-jit"
        if argv.is_empty() {
            let prog = CString::new("hut-jit").unwrap();
            argv.push(prog.as_ptr());
        }

        let argc = argv.len() as c_int;
        let exit_code = unsafe { main(argc, argv.as_ptr()) };

        Ok(exit_code)
    }
}

impl Drop for Tcc {
    fn drop(&mut self) {
        unsafe {
            (self.delete)(self.state);
        }
    }
}
