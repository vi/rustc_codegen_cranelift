#![feature(lang_items, rustc_private)]

#[lang = "start"]
#[inline]
fn lang_start(
    _main: fn(),
    _argc: isize,
    _argv: *const *const u8,
) -> isize {
    std::rt::lang_start_internal()
}

fn main() {}
