//! Compile a WebAssembly text file (.wat) to a binary (.wasm) for the runner.
//!
//! The runner accepts only binaries. This tool is for the examples and for
//! writing tests. It is not part of the shipped product.
//!
//! Usage: cargo run -p tpt-secure-run --example compile_wat -- SOURCE.wat OUTPUT.wasm

use std::fs;

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let [_, source, output] = args.as_slice() else {
        eprintln!("usage: compile_wat SOURCE.wat OUTPUT.wasm");
        std::process::exit(2);
    };
    let text = fs::read_to_string(source).unwrap_or_else(|e| {
        eprintln!("error: cannot read {source}: {e}");
        std::process::exit(1);
    });
    let bytes = wat::parse_str(&text).unwrap_or_else(|e| {
        eprintln!("error: {source}: {e}");
        std::process::exit(2);
    });
    fs::write(output, &bytes).unwrap_or_else(|e| {
        eprintln!("error: cannot write {output}: {e}");
        std::process::exit(1);
    });
    println!("wrote {output} ({} bytes)", bytes.len());
}
