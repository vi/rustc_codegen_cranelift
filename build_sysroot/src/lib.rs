#![no_std]

#![feature(core_intrinsics)]
#![feature(lang_items)]
#![feature(untagged_unions)]
#![feature(never_type)]

extern crate libc;

pub use core::prelude;

pub mod panic {
    #[inline]
    pub fn catch_unwind() -> isize {
        let _a = "" as *const str; // Comment to fix bug
        unsafe { crate::panicking::r#try(crate::rt::c) }
    }
}

mod panicking {
    use core::panic::PanicInfo;
    use core::intrinsics;
    use core::mem::ManuallyDrop;

    pub unsafe fn r#try<F: FnOnce() -> isize>(f: F) -> isize {
        union Data {
            f: ManuallyDrop<()>,
            r: isize,
        }

        let mut data = Data { f: ManuallyDrop::new(()) };

        libc::puts("before do_try\0" as *const str as *const u8);

        intrinsics::r#try(do_call::<()>, 0 as *mut u8, do_catch);
        intrinsics::abort();

        fn do_call<F>(data: *mut u8) {
            unsafe { libc::puts("do_call\0" as *const str as *const u8); }
            unsafe { intrinsics::abort(); }
        }

        fn do_catch(_data: *mut u8, _payload: *mut u8) {
            unsafe { intrinsics::abort(); }
        }
    }

    #[panic_handler]
    pub fn begin_panic_handler(_info: &PanicInfo<'_>) -> ! {
        unsafe { intrinsics::abort(); }
    }
}

pub mod rt {
    pub fn lang_start_internal() -> isize {
        let exit_code = crate::panic::catch_unwind();
        exit_code
    }

    pub fn c() -> isize {
        unsafe { core::intrinsics::abort(); }
    }
}
