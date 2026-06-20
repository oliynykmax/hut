// ── JIT compilation via libtcc ──────────────────────────────────────────────
//
// Optional runtime loading of libtcc.so for in-process C JIT execution.
// Falls back gracefully if libtcc is not installed.

use std::ffi::{CString, c_char, c_int, c_void};
use std::path::Path;

use anyhow::{Context, bail};

type TCCState = c_void;

/// A wrapper around a dynamically-loaded libtcc instance.
pub struct Tcc {
    _lib: libloading::Library,
    state: *mut TCCState,
    delete_fn: unsafe extern "C" fn(*mut TCCState),
    compile_string_fn: unsafe extern "C" fn(*mut TCCState, *const c_char) -> c_int,
    relocate_fn: unsafe extern "C" fn(*mut TCCState, *const c_void) -> c_int,
    get_symbol_fn: unsafe extern "C" fn(*mut TCCState, *const c_char) -> *mut c_void,
}

unsafe impl Send for Tcc {}

impl Tcc {
    /// Try to load libtcc.so from the system.
    /// Returns `None` if TCC is not installed.
    pub fn new() -> Option<Self> {
        // Prefer unversioned .so first, then .so.0, then absolute paths.
        // Arch: /usr/lib/libtcc.so, Debian: /usr/lib/x86_64-linux-gnu/libtcc.so
        let candidates = [
            "libtcc.so",
            "libtcc.so.0",
            "/usr/lib/libtcc.so",
            "/usr/lib64/libtcc.so",
            "/usr/local/lib/libtcc.so",
            "/usr/lib/x86_64-linux-gnu/libtcc.so",
        ];

        let lib = candidates.iter().find_map(|p| {
            if p.starts_with('/') && !Path::new(p).exists() {
                return None;
            }
            unsafe { libloading::Library::new(*p).ok() }
        })?;

        unsafe {
            let new_fn: unsafe extern "C" fn() -> *mut TCCState = *lib.get(b"tcc_new").ok()?;
            let state = new_fn();
            if state.is_null() {
                return None;
            }

            // Look up tcc_set_output_type. On some TCC versions the default is
            // TCC_OUTPUT_EXE, not TCC_OUTPUT_MEMORY, so always set it explicitly.
            // TCC_OUTPUT_MEMORY == 1 in the mob branch, 0 in upstream 0.9.27.
            let set_output_type_fn: unsafe extern "C" fn(*mut TCCState, c_int) -> c_int =
                *lib.get(b"tcc_set_output_type").ok()?;

            // Set output type to TCC_OUTPUT_MEMORY (1 on mob, 0 on upstream 0.9.27).
            // Try both values: mob branch uses 1, upstream uses 0.
            // We try 1 first (mob), then 0 (upstream) if 1 fails.
            if set_output_type_fn(state, 1) != 0 {
                set_output_type_fn(state, 0);
            }

            let delete_fn: unsafe extern "C" fn(*mut TCCState) = *lib.get(b"tcc_delete").ok()?;
            let compile_string_fn: unsafe extern "C" fn(*mut TCCState, *const c_char) -> c_int =
                *lib.get(b"tcc_compile_string").ok()?;
            let relocate_fn: unsafe extern "C" fn(*mut TCCState, *const c_void) -> c_int =
                *lib.get(b"tcc_relocate").ok()?;
            let get_symbol_fn: unsafe extern "C" fn(*mut TCCState, *const c_char) -> *mut c_void =
                *lib.get(b"tcc_get_symbol").ok()?;

            Some(Tcc {
                _lib: lib,
                state,
                delete_fn,
                compile_string_fn,
                relocate_fn,
                get_symbol_fn,
            })
        }
    }

    /// Compile a C source string.
    pub fn compile(&mut self, source: &str) -> anyhow::Result<()> {
        let c_source = CString::new(source).context("Source contained null bytes")?;
        let ret = unsafe { (self.compile_string_fn)(self.state, c_source.as_ptr()) };
        if ret == -1 {
            bail!("TCC compilation failed");
        }
        Ok(())
    }

    /// Relocate and finalize the compiled code.
    pub fn relocate(&mut self) -> anyhow::Result<()> {
        let ret = unsafe { (self.relocate_fn)(self.state, std::ptr::null()) };
        if ret == -1 {
            bail!("TCC relocation failed");
        }
        Ok(())
    }

    /// Get a symbol address (e.g., `main` function pointer).
    pub fn get_symbol(&self, name: &str) -> Option<*mut c_void> {
        let c_name = CString::new(name).ok()?;
        let ptr = unsafe { (self.get_symbol_fn)(self.state, c_name.as_ptr()) };
        if ptr.is_null() { None } else { Some(ptr) }
    }

    /// Run the compiled code by calling `main(argc, argv)`.
    pub fn run_main(&mut self, args: &[String]) -> anyhow::Result<i32> {
        let main_ptr = self
            .get_symbol("main")
            .context("No `main` symbol found in JIT-compiled code")?;

        type MainFn = unsafe extern "C" fn(c_int, *const *const c_char) -> c_int;
        let main: MainFn = unsafe { std::mem::transmute(main_ptr) };

        let c_args: Vec<CString> = args
            .iter()
            .map(|a| CString::new(a.as_str()).unwrap())
            .collect();
        let mut argv: Vec<*const c_char> = c_args.iter().map(|a| a.as_ptr()).collect();

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
            (self.delete_fn)(self.state);
        }
    }
}

// ── Tests ───────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_tcc_new_does_not_panic() {
        let _ = Tcc::new();
    }

    #[test]
    fn test_compile_trivial() {
        let mut tcc = match Tcc::new() {
            Some(t) => t,
            None => return,
        };
        tcc.compile("int add(int a, int b) { return a + b; }")
            .expect("compile");
    }

    #[test]
    fn test_compile_error() {
        let mut tcc = match Tcc::new() {
            Some(t) => t,
            None => return,
        };
        assert!(tcc.compile("int main() { return @#$%; }").is_err());
    }

    #[test]
    fn test_relocate_and_get_symbol() {
        let mut tcc = match Tcc::new() {
            Some(t) => t,
            None => return,
        };
        tcc.compile("int answer(void) { return 42; }").unwrap();
        tcc.relocate().unwrap();
        assert!(tcc.get_symbol("answer").is_some());
    }

    #[test]
    fn test_run_main_exit_code() {
        let mut tcc = match Tcc::new() {
            Some(t) => t,
            None => return,
        };
        tcc.compile("int main(void) { return 42; }").unwrap();
        tcc.relocate().unwrap();
        assert_eq!(tcc.run_main(&[]).unwrap(), 42);
    }

    #[test]
    fn test_run_main_with_args() {
        let mut tcc = match Tcc::new() {
            Some(t) => t,
            None => return,
        };
        tcc.compile("int main(int argc, char** argv) { return argc - 1; }")
            .unwrap();
        tcc.relocate().unwrap();
        let args: Vec<String> = vec!["prog".into(), "a".into(), "b".into()];
        assert_eq!(tcc.run_main(&args).unwrap(), 2);
    }

    #[test]
    fn test_hello_world() {
        let mut tcc = match Tcc::new() {
            Some(t) => t,
            None => return,
        };
        tcc.compile("#include <stdio.h>\nint main(void) { printf(\"hi\\n\"); return 0; }")
            .unwrap();
        tcc.relocate().unwrap();
        assert_eq!(tcc.run_main(&[]).unwrap(), 0);
    }
}
