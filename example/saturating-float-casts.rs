#![feature(compiler_builtins_lib)]

extern crate compiler_builtins;

use std::f32;

use compiler_builtins::float::Float;
use compiler_builtins::int::Int;

// cranelift doesn't inline
fn black_box<T>(a: T) -> T {
    a
}


#[derive(PartialEq)]
enum Sign {
    Positive,
    Negative,
}

macro_rules! float_to_int {
    ($f:expr, $fty:ty, $ity:ty) => {{
        let f = $f;
        let fixint_min = <$ity>::min_value();
        let fixint_max = <$ity>::max_value();
        let fixint_bits = <$ity>::BITS as usize;
        let fixint_unsigned = fixint_min == 0;

        let sign_bit = <$fty>::SIGN_MASK;
        let significand_bits = <$fty>::SIGNIFICAND_BITS as usize;
        let exponent_bias = <$fty>::EXPONENT_BIAS as usize;
        //let exponent_max = <$fty>::exponent_max() as usize;

        // Break a into sign, exponent, significand
        let a_rep = <$fty>::repr(f);
        let a_abs = a_rep & !sign_bit;

        // this is used to work around -1 not being available for unsigned
        let sign = if (a_rep & sign_bit) == 0 {
            Sign::Positive
        } else {
            Sign::Negative
        };
        let mut exponent = (a_abs >> significand_bits) as usize;
        let significand = (a_abs & <$fty>::SIGNIFICAND_MASK) | <$fty>::IMPLICIT_BIT;

        // if < 1 or unsigned & negative
        if exponent < exponent_bias || fixint_unsigned && sign == Sign::Negative {
            return 0;
        }
        exponent -= exponent_bias;

        // If the value is infinity, saturate.
        // If the value is too large for the integer type, 0.
        if exponent
            >= (if fixint_unsigned {
                fixint_bits
            } else {
                fixint_bits - 1
            })
        {
            return if sign == Sign::Positive {
                fixint_max
            } else {
                fixint_min
            };
        }
        // If 0 <= exponent < significand_bits, right shift to get the result.
        // Otherwise, shift left.
        // (sign - 1) will never overflow as negative signs are already returned as 0 for unsigned
        let r = if exponent < significand_bits {
            (significand >> (significand_bits - exponent)) as $ity
        } else {
            (significand as $ity) << (exponent - significand_bits)
        };

        if sign == Sign::Negative {
            (!r).wrapping_add(1)
        } else {
            r
        }
    }};
}

fn f32_to_i128(f: f32) -> i128 {
    float_to_int!(f, f32, i128)
}

fn main() {
    assert_eq!(f32_to_i128(f32::NAN), 0i128);
}
