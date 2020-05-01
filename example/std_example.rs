#![feature(core_intrinsics, generators, generator_trait, is_sorted)]

use std::ops::Shl;

fn main() {
    #[derive(Copy, Clone)]
    enum Nums {
        NegOne = -1,
    }

    let kind = Nums::NegOne;
    assert_eq!(-1i128, kind as i128);

    if u8::shl(1, 9) != 2_u8 {
        unsafe { std::intrinsics::abort(); }
    }

    /*const STR: &'static str = "hello";
    fn other_casts() -> *const str {
        STR as *const str
    }
    if other_casts() != STR as *const str {
        unsafe { std::intrinsics::abort(); }
    }*/

}
