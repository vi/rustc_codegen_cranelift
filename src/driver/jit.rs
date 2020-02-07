use std::cell::RefCell;
use std::ffi::CString;
use std::os::raw::{c_char, c_int};

use rustc_codegen_ssa::CrateInfo;

use cranelift_simplejit::{SimpleJITBackend, SimpleJITBuilder};

use crate::prelude::*;

thread_local! {
    pub static EXISTING_SYMBOLS: RefCell<FxHashMap<String, *const u8>> = RefCell::new(FxHashMap::default());
    pub static INSTANCE_CODEGEN: RefCell<FxHashMap<Instance<'static>, *const u8>> = RefCell::new(FxHashMap::default());
}

fn make_jit(tcx: TyCtxt<'_>) -> Module<SimpleJITBackend> {
    let existing_symbols: Vec<_> = EXISTING_SYMBOLS.with(|existing_symbols| {
        existing_symbols.borrow().iter().map(|(name, &addr)| (name.clone(), addr)).collect()
    });

    let mut jit_builder = SimpleJITBuilder::with_isa(
        crate::build_isa(tcx.sess, false),
        cranelift_module::default_libcall_names(),
    );
    jit_builder.symbols(existing_symbols);
    let jit_module = Module::new(jit_builder);
    assert_eq!(pointer_ty(tcx), jit_module.target_config().pointer_type());
    jit_module
}

pub(super) fn run_jit(tcx: TyCtxt<'_>) -> ! {
    // Rustc opens us without the RTLD_GLOBAL flag, so __cg_clif_global_atomic_mutex will not be
    // exported. We fix this by opening ourself again as global.
    // FIXME remove once atomic_shim is gone
    let cg_dylib = std::ffi::OsString::from(&tcx.sess.opts.debugging_opts.codegen_backend.as_ref().unwrap());
    std::mem::forget(libloading::os::unix::Library::open(Some(cg_dylib), libc::RTLD_NOW | libc::RTLD_GLOBAL).unwrap());

    EXISTING_SYMBOLS.with(|existing_symbols| {
        assert!(existing_symbols.borrow().is_empty());
        *existing_symbols.borrow_mut() = load_imported_symbols_for_jit(tcx);
    });

    let mut jit_module = make_jit(tcx);

    let sig = Signature {
        params: vec![
            AbiParam::new(jit_module.target_config().pointer_type()),
            AbiParam::new(jit_module.target_config().pointer_type()),
        ],
        returns: vec![AbiParam::new(
            jit_module.target_config().pointer_type(), /*isize*/
        )],
        call_conv: CallConv::triple_default(&crate::target_triple(tcx.sess)),
    };
    let main_func_id = jit_module
        .declare_function("main", Linkage::Import, &sig)
        .unwrap();

    if !tcx.sess.opts.output_types.should_codegen() {
        tcx.sess.fatal("JIT mode doesn't work with `cargo check`.");
    }

    let (_, cgus) = tcx.collect_and_partition_mono_items(LOCAL_CRATE);
    let mono_items = cgus
        .iter()
        .map(|cgu| cgu.items_in_deterministic_order(tcx).into_iter())
        .flatten()
        .collect::<FxHashMap<_, (_, _)>>()
        .into_iter()
        .collect::<Vec<(_, (_, _))>>();

    let mut cx = CodegenCx::new(tcx, jit_module, false, true);

    let (mut jit_module, _debug, mut unwind_context) = super::time(tcx, "codegen mono items", || {
        super::codegen_mono_items(&mut cx, mono_items, true);
        tcx.sess.time("finalize CodegenCx", || cx.finalize())
    });
    crate::main_shim::maybe_create_entry_wrapper(tcx, &mut jit_module, &mut unwind_context);
    crate::allocator::codegen(tcx, &mut jit_module, &mut unwind_context);

    jit_module.finalize_definitions();

    let _unwind_register_guard = unsafe { unwind_context.register_jit(&mut jit_module) };

    tcx.sess.abort_if_errors();

    let finalized_main: *const u8 = jit_module.get_finalized_function(main_func_id);

    println!("Rustc codegen cranelift will JIT run the executable, because the CG_CLIF_JIT env var is set");

    let f: extern "C" fn(c_int, *const *const c_char) -> c_int =
        unsafe { ::std::mem::transmute(finalized_main) };

    let args = ::std::env::var("CG_CLIF_JIT_ARGS").unwrap_or_else(|_| String::new());
    let args = std::iter::once(&*tcx.crate_name(LOCAL_CRATE).as_str().to_string())
        .chain(args.split(" "))
        .map(|arg| CString::new(arg).unwrap())
        .collect::<Vec<_>>();
    let argv = args.iter().map(|arg| arg.as_ptr()).collect::<Vec<_>>();
    // TODO: Rust doesn't care, but POSIX argv has a NULL sentinel at the end

    let ret = f(args.len() as c_int, argv.as_ptr());

    jit_module.finish();
    std::process::exit(ret);
}

#[no_mangle]
pub extern "C" fn __clif_jit_fn(instance_ptr: *const Instance<'static>) -> *const u8 {
    if let Some(f) = INSTANCE_CODEGEN.with(|instance_codegen| {
        if let Some(&f) = instance_codegen.borrow().get(unsafe { &*instance_ptr }) {
            println!("Existing @ {:p}", f);
            Some(f)
        } else {
            None
        }
    }) {
        return f;
    }

    rustc_middle::ty::tls::with(|tcx| {
        // lift is used to ensure the correct lifetime for instance.
        let instance = tcx.lift(unsafe { &*instance_ptr }).unwrap();

        let jit_module = make_jit(tcx);
        let mut cx = CodegenCx::new(tcx, jit_module, false, false);

        super::trans_mono_item(&mut cx, rustc_middle::mir::mono::MonoItem::Fn(instance), Linkage::Export, false);

        let (name, sig) = crate::abi::get_function_name_and_sig(tcx, cx.module.isa().triple(), instance, true);
        let func_id = cx.module
            .declare_function(&name, Linkage::Export, &sig)
            .unwrap();

        let (mut jit_module, _debug_context, _unwind_context) = cx.finalize();
        jit_module.finalize_definitions();
        let ptr = jit_module.get_finalized_function(func_id);
        jit_module.finish();

        INSTANCE_CODEGEN.with(|instance_codegen| {
            instance_codegen.borrow_mut().insert(unsafe { (&*instance_ptr).clone() }, ptr);
        });

        EXISTING_SYMBOLS.with(|existing_symbols| {
            existing_symbols.borrow_mut().insert(name, ptr);
        });

        println!("New {:?} @ {:p}", instance, ptr);

        ptr
    })
}

fn load_imported_symbols_for_jit(tcx: TyCtxt<'_>) -> FxHashMap<String, *const u8> {
    use rustc_middle::middle::dependency_format::Linkage;

    let mut dylib_paths = Vec::new();

    let crate_info = CrateInfo::new(tcx);
    let formats = tcx.dependency_formats(LOCAL_CRATE);
    let data = &formats
        .iter()
        .find(|(crate_type, _data)| *crate_type == rustc_session::config::CrateType::Executable)
        .unwrap()
        .1;
    for &(cnum, _) in &crate_info.used_crates_dynamic {
        let src = &crate_info.used_crate_source[&cnum];
        match data[cnum.as_usize() - 1] {
            Linkage::NotLinked | Linkage::IncludedFromDylib => {}
            Linkage::Static => {
                let name = tcx.crate_name(cnum);
                let mut err = tcx
                    .sess
                    .struct_err(&format!("Can't load static lib {}", name.as_str()));
                err.note("rustc_codegen_cranelift can only load dylibs in JIT mode.");
                err.emit();
            }
            Linkage::Dynamic => {
                dylib_paths.push(src.dylib.as_ref().unwrap().0.clone());
            }
        }
    }

    let mut imported_symbols = FxHashMap::default();
    for path in dylib_paths {
        use object::Object;
        let lib = libloading::Library::new(&path).unwrap();
        let obj = std::fs::read(path).unwrap();
        let obj = object::File::parse(&obj).unwrap();
        imported_symbols.extend(obj.dynamic_symbols().filter_map(|(_idx, symbol)| {
            let name = symbol.name().unwrap().to_string();
            if name.is_empty() || !symbol.is_global() || symbol.is_undefined() {
                return None;
            }
            let dlsym_name = if cfg!(target_os = "macos") {
                // On macOS `dlsym` expects the name without leading `_`.
                assert!(name.starts_with("_"), "{:?}", name);
                &name[1..]
            } else {
                &name
            };
            let symbol: libloading::Symbol<'_, *const u8> =
                unsafe { lib.get(dlsym_name.as_bytes()) }.unwrap();
            Some((name, *symbol))
        }));
        std::mem::forget(lib)
    }

    tcx.sess.abort_if_errors();

    imported_symbols
}


pub(super) fn codegen_shim<'tcx>(cx: &mut CodegenCx<'tcx, impl Backend>, inst: Instance<'tcx>) {
    let tcx = cx.tcx;

    let pointer_type = cx.module.target_config().pointer_type();

    let (name, sig) = crate::abi::get_function_name_and_sig(tcx, cx.module.isa().triple(), inst, true);
    let func_id = cx.module
        .declare_function(&name, Linkage::Export, &sig)
        .unwrap();

    let instance_ptr = Box::into_raw(Box::new(inst));

    let jit_fn = cx.module.declare_function("__clif_jit_fn", Linkage::Import, &Signature {
        call_conv: cx.module.target_config().default_call_conv,
        params: vec![AbiParam::new(pointer_type)],
        returns: vec![AbiParam::new(pointer_type)],
    }).unwrap();

    let mut trampoline = Function::with_name_signature(ExternalName::default(), sig.clone());
    let mut builder_ctx = FunctionBuilderContext::new();
    let mut trampoline_builder = FunctionBuilder::new(&mut trampoline, &mut builder_ctx);

    let jit_fn = cx.module.declare_func_in_func(jit_fn, trampoline_builder.func);
    let sig_ref = trampoline_builder.func.import_signature(sig);

    let entry_block = trampoline_builder.create_block();
    trampoline_builder.append_block_params_for_function_params(entry_block);
    let fn_args = trampoline_builder.func.dfg.block_params(entry_block).to_vec();

    trampoline_builder.switch_to_block(entry_block);
    let instance_ptr = trampoline_builder.ins().iconst(pointer_type, instance_ptr as u64 as i64);
    let jitted_fn = trampoline_builder.ins().call(jit_fn, &[instance_ptr]);
    let jitted_fn = trampoline_builder.func.dfg.inst_results(jitted_fn)[0];
    let call_inst = trampoline_builder.ins().call_indirect(sig_ref, jitted_fn, &fn_args);
    let ret_vals = trampoline_builder.func.dfg.inst_results(call_inst).to_vec();
    trampoline_builder.ins().return_(&ret_vals);

    cx.module.define_function(func_id, &mut Context::for_function(trampoline), &mut cranelift_codegen::binemit::NullTrapSink {}).unwrap();
}
