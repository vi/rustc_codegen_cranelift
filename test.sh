#!/bin/bash

set -e

if [[ "$1" == "--release" ]]; then
    export CHANNEL='release'
    cargo build --release $CG_CLIF_COMPILE_FLAGS
else
    export CHANNEL='debug'
    cargo build $CG_CLIF_COMPILE_FLAGS
fi

source config.sh

jit() {
    if [[ `uname` == 'Darwin' ]]; then
        # FIXME(#671) `dlsym` returns "symbol not found" for existing symbols on macOS.
        echo "[JIT] $1 (Ignored on macOS)"
    else
        echo "[JIT] $1"
        SHOULD_RUN=1 $RUSTC --crate-type bin -Cprefer-dynamic $2
    fi
}

rm -r target/out || true
mkdir -p target/out/clif

echo "[BUILD] mini_core"
$RUSTC example/mini_core.rs --crate-name mini_core --crate-type lib,dylib

echo "[BUILD] example"
$RUSTC example/example.rs --crate-type lib

JIT_ARGS="abc bcd" jit mini_core_hello_world example/mini_core_hello_world.rs

echo "[AOT] mini_core_hello_world"
$RUSTC example/mini_core_hello_world.rs --crate-name mini_core_hello_world --crate-type bin
./target/out/mini_core_hello_world abc bcd

echo "[AOT] arbitrary_self_types_pointers_and_wrappers"
$RUSTC example/arbitrary_self_types_pointers_and_wrappers.rs --crate-type bin -Cpanic=abort
./target/out/arbitrary_self_types_pointers_and_wrappers

echo "[BUILD] sysroot"
time ./build_sysroot/build_sysroot.sh

$RUSTC example/std_example.rs --crate-type bin
./target/out/std_example

git clone https://github.com/rust-lang/rust.git --depth=1 || true
cd rust
git checkout -- .
#git pull
export RUSTFLAGS=

cat > the_patch.patch <<EOF
From 681aa334c5c183538e77c660e5e2d4d0c79fe669 Mon Sep 17 00:00:00 2001
From: bjorn3 <bjorn3@users.noreply.github.com>
Date: Sat, 23 Feb 2019 14:55:44 +0100
Subject: [PATCH] Make suitable for cg_clif tests

---
 .gitmodules           | 10 ----------
 1 files changed, 0 insertions(+), 10 deletions(-)

diff --git a/.gitmodules b/.gitmodules
index 31db0772cfb..5832ba2578b 100644
--- a/.gitmodules
+++ b/.gitmodules
@@ -25,9 +25,6 @@
 [submodule "src/tools/miri"]
 	path = src/tools/miri
 	url = https://github.com/rust-lang/miri.git
-[submodule "src/doc/rust-by-example"]
-	path = src/doc/rust-by-example
-	url = https://github.com/rust-lang/rust-by-example.git
 [submodule "src/stdarch"]
 	path = src/stdarch
 	url = https://github.com/rust-lang/stdarch.git
@@ -37,10 +34,6 @@
 [submodule "src/doc/edition-guide"]
 	path = src/doc/edition-guide
 	url = https://github.com/rust-lang/edition-guide.git
-[submodule "src/llvm-project"]
-	path = src/llvm-project
-	url = https://github.com/rust-lang/llvm-project.git
-	branch = rustc/9.0-2019-09-19
 [submodule "src/doc/embedded-book"]
 	path = src/doc/embedded-book
 	url = https://github.com/rust-embedded/book.git
diff --git a/src/liballoc/lib.rs b/src/liballoc/lib.rs
index ddfa6797a57..94379afc2bd 100644
--- a/src/liballoc/lib.rs
+++ b/src/liballoc/lib.rs
@@ -116,7 +116,7 @@
 #![feature(unsize)]
 #![feature(unsized_locals)]
 #![feature(allocator_internals)]
-#![cfg_attr(bootstrap, feature(on_unimplemented))]
+#![feature(on_unimplemented)]
 #![feature(rustc_const_unstable)]
 #![feature(slice_partition_dedup)]
 #![feature(maybe_uninit_extra, maybe_uninit_slice)]
diff --git a/src/libcore/lib.rs b/src/libcore/lib.rs
index ca431627147..1b67b05c730 100644
--- a/src/libcore/lib.rs
+++ b/src/libcore/lib.rs
@@ -89,7 +89,7 @@
 #![feature(nll)]
 #![feature(exhaustive_patterns)]
 #![feature(no_core)]
-#![cfg_attr(bootstrap, feature(on_unimplemented))]
+#![feature(on_unimplemented)]
 #![feature(optin_builtin_traits)]
 #![feature(prelude_import)]
 #![feature(repr_simd, platform_intrinsics)]
diff --git a/src/libstd/lib.rs b/src/libstd/lib.rs
index 927fd2a6b0b..c7adad896a5 100644
--- a/src/libstd/lib.rs
+++ b/src/libstd/lib.rs
@@ -284,7 +284,7 @@
 #![feature(never_type)]
 #![feature(nll)]
 #![cfg_attr(bootstrap, feature(non_exhaustive))]
-#![cfg_attr(bootstrap, feature(on_unimplemented))]
+#![feature(on_unimplemented)]
 #![feature(optin_builtin_traits)]
 #![feature(panic_info_message)]
 #![feature(panic_internals)]
diff --git a/src/tools/compiletest/src/main.rs b/src/tools/compiletest/src/main.rs
index 34435819a2c..b115539b4af 100644
--- a/src/tools/compiletest/src/main.rs
+++ b/src/tools/compiletest/src/main.rs
@@ -568,6 +568,7 @@ pub fn test_opts(config: &Config) -> test::TestOpts {
         skip: vec![],
         list: false,
         options: test::Options::new(),
+        time_options: None,
     }
 }

@@ -703,6 +704,7 @@ pub fn make_test(config: &Config, testpaths: &TestPaths) -> Vec<test::TestDescAn
                     ignore,
                     should_panic,
                     allow_fail: false,
+                    test_type: test::TestType::Unknown,
                 },
                 testfn: make_test_closure(config, early_props.ignore, testpaths, revision),
             }
diff --git a/src/tools/compiletest/src/runtest.rs b/src/tools/compiletest/src/runtest.rs
index a9acc9733c1..302b7ef8e76 100644
--- a/src/tools/compiletest/src/runtest.rs
+++ b/src/tools/compiletest/src/runtest.rs
@@ -1798,6 +1798,7 @@ impl<'test> TestCx<'test> {
                 || self.config.target.contains("wasm32")
                 || self.config.target.contains("nvptx")
                 || self.is_vxworks_pure_static()
+                || true
             {
                 // We primarily compile all auxiliary libraries as dynamic libraries
                 // to avoid code size bloat and large binaries as much as possible
@@ -2033,7 +2034,7 @@ impl<'test> TestCx<'test> {
                 || self.is_vxworks_pure_static() {
                 // rustc.arg("-g"); // get any backtrace at all on errors
             } else if !self.props.no_prefer_dynamic {
-                rustc.args(&["-C", "prefer-dynamic"]);
+                // rustc.args(&["-C", "prefer-dynamic"]);
             }
         }

--
2.11.0

EOF

git apply the_patch.patch

rm config.toml || true

cat > config.toml <<EOF
[rust]
codegen-backends = []
[build]
local-rebuild = true
rustc = "$HOME/.rustup/toolchains/nightly-x86_64-unknown-linux-gnu/bin/rustc"
EOF

rm -r src/test/run-pass/{asm-*,abi-*,extern/,panic-runtime/,panics/,unsized-locals/,proc-macro/,threads-sendsync/,thinlto/,simd/} || true
for test in src/test/run-pass/*.rs src/test/run-pass/**/*.rs; do
    if grep "ignore-emscripten" $test 2>&1 >/dev/null; then
        rm $test
    fi
done

echo "[TEST] run-pass"

#rm -r build/x86_64-unknown-linux-gnu/test || true
./x.py test --stage 0 src/test/ui/ \
    --rustc-args "-Zcodegen-backend=$(pwd)/../target/"$CHANNEL"/librustc_codegen_cranelift."$dylib_ext" --sysroot $(pwd)/../build_sysroot/sysroot -Cpanic=abort" \
    2>&1 | tee log.txt
