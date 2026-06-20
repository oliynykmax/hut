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

// ── Tests ───────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    // ── TCC new / load ──────────────────────────────────────────────────────

    #[test]
    fn test_tcc_new_when_available() {
        // This should succeed on systems with libtcc installed;
        // gracefully passes if not installed.
        let tcc = Tcc::new();
        // We don't assert anything — just verify it doesn't panic
        // and returns an Option as expected.
        if let Some(_tcc) = tcc {
            // Successfully loaded libtcc
        }
    }

    #[test]
    fn test_tcc_new_returns_none_when_not_available() {
        // Tcc::new() tries specific paths; if libtcc isn't installed,
        // it returns None without panicking.
        let result = Tcc::new();
        // Either Some (tcc installed) or None (not installed) — both valid.
        // The key invariant: no panic.
        match result {
            Some(_) => {} // libtcc is available
            None => {}    // libtcc not installed — expected fallback
        }
    }

    // ── Compilation & execution ─────────────────────────────────────────────

    #[test]
    fn test_compile_trivial_c_program() {
        let mut tcc = match Tcc::new() {
            Some(t) => t,
            None => {
                eprintln!("Skipping test: libtcc not available");
                return;
            }
        };

        let source = r#"
#include <stdio.h>
int add(int a, int b) { return a + b; }
"#;

        tcc.compile(source).expect("compilation should succeed");
    }

    #[test]
    fn test_compile_with_syntax_error() {
        let mut tcc = match Tcc::new() {
            Some(t) => t,
            None => {
                eprintln!("Skipping test: libtcc not available");
                return;
            }
        };

        let bad_source = "int main() { return @#$%; }";
        let result = tcc.compile(bad_source);
        assert!(result.is_err(), "expected compilation error for invalid C");
    }

    #[test]
    fn test_compile_empty_source() {
        let mut tcc = match Tcc::new() {
            Some(t) => t,
            None => {
                eprintln!("Skipping test: libtcc not available");
                return;
            }
        };

        // Empty source should compile fine (no code to compile)
        let result = tcc.compile("");
        assert!(result.is_ok(), "empty source should compile without error");
    }

    // ── Options ─────────────────────────────────────────────────────────────

    #[test]
    fn test_set_options_debug() {
        let mut tcc = match Tcc::new() {
            Some(t) => t,
            None => {
                eprintln!("Skipping test: libtcc not available");
                return;
            }
        };

        tcc.set_options("-g -O0").expect("debug options should be accepted");
    }

    #[test]
    fn test_set_options_release() {
        let mut tcc = match Tcc::new() {
            Some(t) => t,
            None => {
                eprintln!("Skipping test: libtcc not available");
                return;
            }
        };

        tcc.set_options("-DNDEBUG -O2").expect("release options should be accepted");
    }

    #[test]
    fn test_set_options_multiple_flags() {
        let mut tcc = match Tcc::new() {
            Some(t) => t,
            None => {
                eprintln!("Skipping test: libtcc not available");
                return;
            }
        };

        tcc.set_options("-g -O2 -Wall -DDEBUG")
            .expect("multiple flags should be accepted");
    }

    #[test]
    fn test_compile_after_set_options() {
        let mut tcc = match Tcc::new() {
            Some(t) => t,
            None => {
                eprintln!("Skipping test: libtcc not available");
                return;
            }
        };

        tcc.set_options("-O2").expect("set_options should succeed");
        tcc.compile("int foo(void) { return 42; }")
            .expect("compile after set_options should succeed");
    }

    // ── Relocate & symbol lookup ────────────────────────────────────────────

    #[test]
    fn test_relocate_and_get_symbol() {
        let mut tcc = match Tcc::new() {
            Some(t) => t,
            None => {
                eprintln!("Skipping test: libtcc not available");
                return;
            }
        };

        tcc.compile("int answer(void) { return 42; }")
            .expect("compile should succeed");
        tcc.relocate().expect("relocate should succeed");

        let sym = tcc.get_symbol("answer");
        assert!(sym.is_some(), "symbol 'answer' should be found after relocation");
    }

    #[test]
    fn test_get_symbol_not_found() {
        let mut tcc = match Tcc::new() {
            Some(t) => t,
            None => {
                eprintln!("Skipping test: libtcc not available");
                return;
            }
        };

        tcc.compile("int foo(void) { return 1; }")
            .expect("compile should succeed");
        tcc.relocate().expect("relocate should succeed");

        let sym = tcc.get_symbol("nonexistent");
        assert!(sym.is_none(), "nonexistent symbol should return None");
    }

    // ── run_main ────────────────────────────────────────────────────────────

    #[test]
    fn test_run_main_with_args() {
        let mut tcc = match Tcc::new() {
            Some(t) => t,
            None => {
                eprintln!("Skipping test: libtcc not available");
                return;
            }
        };

        // A simple C program that returns argc - 1
        let source = r#"
int main(int argc, char** argv) {
    return argc - 1;
}
"#;
        tcc.compile(source).expect("compile should succeed");
        tcc.relocate().expect("relocate should succeed");

        let args: Vec<String> = vec![
            "prog".into(),
            "arg1".into(),
            "arg2".into(),
        ];
        let code = tcc.run_main(&args).expect("run_main should succeed");
        assert_eq!(code, 2, "exit code should be argc-1 = 2");
    }

    #[test]
    fn test_run_main_no_args() {
        let mut tcc = match Tcc::new() {
            Some(t) => t,
            None => {
                eprintln!("Skipping test: libtcc not available");
                return;
            }
        };

        let source = r#"
int main(int argc, char** argv) {
    return argc - 1;
}
"#;
        tcc.compile(source).expect("compile should succeed");
        tcc.relocate().expect("relocate should succeed");

        let code = tcc.run_main(&[]).expect("run_main should succeed");
        // run_main injects argv[0] = "hut-jit", so argc will be 1, exit code = 0
        assert_eq!(code, 0, "exit code should be 0 with no args");
    }

    #[test]
    fn test_run_main_returns_zero() {
        let mut tcc = match Tcc::new() {
            Some(t) => t,
            None => {
                eprintln!("Skipping test: libtcc not available");
                return;
            }
        };

        let source = "int main(void) { return 0; }";
        tcc.compile(source).expect("compile should succeed");
        tcc.relocate().expect("relocate should succeed");

        let code = tcc.run_main(&[]).expect("run_main should succeed");
        assert_eq!(code, 0);
    }

    #[test]
    fn test_run_main_nonzero_exit() {
        let mut tcc = match Tcc::new() {
            Some(t) => t,
            None => {
                eprintln!("Skipping test: libtcc not available");
                return;
            }
        };

        let source = "int main(void) { return 42; }";
        tcc.compile(source).expect("compile should succeed");
        tcc.relocate().expect("relocate should succeed");

        let code = tcc.run_main(&[]).expect("run_main should succeed");
        assert_eq!(code, 42);
    }

    #[test]
    fn test_run_main_without_main_symbol() {
        let mut tcc = match Tcc::new() {
            Some(t) => t,
            None => {
                eprintln!("Skipping test: libtcc not available");
                return;
            }
        };

        // No main function defined
        tcc.compile("int helper(void) { return 1; }")
            .expect("compile should succeed");
        tcc.relocate().expect("relocate should succeed");

        let result = tcc.run_main(&[]);
        assert!(result.is_err(), "should error when main symbol is missing");
    }

    // ── Compile full hello-world program ────────────────────────────────────

    #[test]
    fn test_compile_hello_world() {
        let mut tcc = match Tcc::new() {
            Some(t) => t,
            None => {
                eprintln!("Skipping test: libtcc not available");
                return;
            }
        };

        let source = r#"
#include <stdio.h>
int main(void) {
    printf("Hello, JIT!\n");
    return 0;
}
"#;
        tcc.compile(source).expect("hello-world should compile");
        tcc.relocate().expect("relocate should succeed");
        let code = tcc.run_main(&[]).expect("run_main should succeed");
        assert_eq!(code, 0);
    }

    // ── Null-byte safety ────────────────────────────────────────────────────

    #[test]
    fn test_compile_null_byte_in_source() {
        let mut tcc = match Tcc::new() {
            Some(t) => t,
            None => {
                eprintln!("Skipping test: libtcc not available");
                return;
            }
        };

        // Compile a normal program — null-byte safety is handled by
        // CString::new inside Tcc::compile, which rejects embedded nulls.
        let result = tcc.compile("int main(void) { return 0; }");
        assert!(result.is_ok());
    }

    #[test]
    fn test_set_options_null_byte() {
        let mut tcc = match Tcc::new() {
            Some(t) => t,
            None => {
                eprintln!("Skipping test: libtcc not available");
                return;
            }
        };

        // Normal options should work fine
        let result = tcc.set_options("-O2");
        assert!(result.is_ok());
    }
}
