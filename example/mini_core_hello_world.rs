// Adapted from https://github.com/sunfishcode/mir2cranelift/blob/master/rust-examples/nocore-hello-world.rs

#![feature(
    no_core, unboxed_closures, start, lang_items, box_syntax, never_type, linkage,
    extern_types, thread_local
)]
#![no_core]
#![allow(dead_code, non_camel_case_types)]

extern crate mini_core;

use mini_core::*;
use mini_core::libc::*;

#[lang = "termination"]
trait Termination {
    fn report(self) -> i32;
}

impl Termination for () {
    fn report(self) -> i32 {
        0
    }
}

#[lang = "start"]
fn start<T: Termination + 'static>(
    main: fn() -> T,
    argc: isize,
    argv: *const *const u8,
) -> isize {
    main().report();
    0
}

macro_rules! assert_eq {
    ($l:expr, $r: expr) => {
        if $l != $r {
            panic(&(stringify!($l != $r), file!(), line!(), 0));
        }
    }
}

fn main() {
    unsafe {
        assert_eq!(intrinsics::ctlz(0b0000000000000000000000000010000010000000000000000000000000000000_0000000000100000000000000000000000001000000000000100000000000000u128) as u32, 26u32);
    }
}
