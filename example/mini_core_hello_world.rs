// Adapted from https://github.com/sunfishcode/mir2cranelift/blob/master/rust-examples/nocore-hello-world.rs

#![feature(
    no_core, unboxed_closures, start, lang_items, box_syntax, never_type, linkage,
    extern_types, thread_local
)]
#![no_core]
#![no_main]
#![allow(dead_code, non_camel_case_types)]

extern crate mini_core;

use mini_core::*;
use mini_core::libc::*;

#[no_mangle]
pub extern "C" fn main(
    argc: isize,
    argv: *const *const u8,
) -> isize {
    argv as usize + 8;

    0
}
