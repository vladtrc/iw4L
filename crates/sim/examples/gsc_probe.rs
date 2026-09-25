use sim::{gsc_ir::{Catalog, Namespace, NativeRegistry, Program, Value}, *};
use std::collections::BTreeMap;

fn compile(source: &str) -> Program {
    Program::load(&BTreeMap::from([("probe".to_owned(), source.to_owned())]), &["probe"], &Catalog::iw4()).unwrap()
}
fn world(source: &str) -> SimWorld {
    let mut w = SimWorld::new();
    w.install_gsc_program(compile(source), NativeRegistry::default()).unwrap();
    w
}
fn tick(w: &mut SimWorld, n: u32) {
    try_step(w, Tick(n), &TickInput::default(), 50, StepReason::AuthorityFrame).unwrap();
}
fn show(value: &Value) -> String {
    match value {
        Value::Int(n) => format!("int {n}"),
        Value::Float(n) => format!("float {n}"),
        Value::String(s) => format!("string {s:?}"),
        other => format!("{other:?}"),
    }
}
fn logging() -> NativeRegistry {
    let mut natives = NativeRegistry::default();
    natives.register(Namespace::Function, "log", |_, _, args| {
        println!("  log: {}", args.iter().map(show).collect::<Vec<_>>().join(", "));
        Ok(Value::Undefined)
    });
    natives
}
fn run_logged(title: &str, source: &str, ticks: u32) -> Result<(), sim::gsc_ir::Fault> {
    println!("CASE {title}");
    let program = Program::load(&BTreeMap::from([("probe".to_owned(), source.to_owned())]), &["probe"], &Catalog::iw4().with(Namespace::Function, "log"))?;
    let mut w = SimWorld::new();
    w.install_gsc_program(program, logging()).unwrap();
    w.start_gsc("probe::main", Value::Undefined, vec![]).unwrap();
    for n in 1..=ticks {
        try_step(&mut w, Tick(n), &TickInput::default(), 50, StepReason::AuthorityFrame)?;
    }
    Ok(())
}
fn expect_fault(title: &str, source: &str, needle: &str) {
    let fault = run_logged(title, source, 1).unwrap_err();
    println!("  fault: {fault}");
    assert!(fault.message.contains(needle), "{fault}");
}
fn main() {
    let mut empty = SimWorld::new();
    assert!(try_step(&mut empty, Tick(1), &TickInput::default(), 50, StepReason::AuthorityFrame).unwrap_err().message.contains("no loaded"));
    println!("PASS missing program rejects authority step");
    let source = r#"
        main() {
            total = 0; i = 0;
            while (i < 5) { i++; if(i == 2) continue; total += twice(i); if (total == 26) break; }
            if (total != 26) fail();
            if (0 && fail()) fail();
            if (1 || fail()) total = total + 1;
            level.shared = spawnstruct(); alias = level.shared; alias.value = total;
            if(level.shared.value != 27) fail();
            self setorigin((1, 2, 3));
            if(self.origin != (1,2,3)) fail();
            thread listener(); thread sender();
            wait 0.1;
            if(level.payload != 42) fail();
            if(isdefined(level.cancelled)) fail();
            self setorigin((4, 5, 6));
        }
        twice(n) { return n * 2; }
        fail() { return 1 / 0; }
        listener() { level endon("stop"); level waittill("go", payload); level.payload = payload; level waittill("never"); level.cancelled = 1; }
        sender() { level notify("go", 42); wait 0.05; level notify("stop"); }
    "#;
    let mut w = world(source);
    let id = ScriptModelId::from_wire(17);
    let number = w.spawn_script_mover(id, [0.0;3], [0.0;3]).unwrap();
    let receiver = w.entity_kernel().current_ref(number).unwrap();
    w.start_gsc("probe::main", Value::Entity(receiver), Vec::new()).unwrap();
    tick(&mut w, 1);
    assert_eq!(w.script_mover_by_number(number).unwrap().state.tr_base, [1.,2.,3.]);
    let mut clone = w.clone();
    try_step(&mut w, Tick(100), &TickInput::default(), 50, StepReason::PredictNew).unwrap();
    tick(&mut w, 2); tick(&mut clone, 2);
    assert_eq!(w.script_mover_by_number(number).unwrap().state.tr_base, [1.,2.,3.]);
    tick(&mut w, 3); tick(&mut clone, 3);
    assert_eq!(w.script_mover_by_number(number).unwrap().state.tr_base, [4.,5.,6.]);
    assert_eq!(w.script_mover_by_number(number), clone.script_mover_by_number(number));
    println!("PASS calls, loops, short circuit, aliasing, native mover state, wait, payload, endon, clone, prediction isolation");
    let fault = try_step(&mut w, Tick(3), &TickInput::default(), 50, StepReason::AuthorityFrame).unwrap_err();
    assert!(fault.message.contains("ticks must increase"));
    println!("PASS duplicate authority tick faults");
    let mut w = world("main() { wait 0; fail(); } fail() { return 1 / 0; }");
    w.start_gsc("probe::main", Value::Undefined, vec![]).unwrap();
    let error = try_step(&mut w, Tick(1), &TickInput::default(), 50, StepReason::AuthorityFrame).unwrap_err();
    assert_eq!(error.location.function, "fail"); assert_eq!(error.callers.len(), 1);
    assert_eq!(try_step(&mut w, Tick(2), &TickInput::default(), 50, StepReason::AuthorityFrame).unwrap_err(), error);
    println!("PASS wait zero resumes in the same frame, contextual fault is sticky: {error}");
    let mut w = world("main() { while(1) {} }");
    w.start_gsc("probe::main", Value::Undefined, vec![]).unwrap();
    assert!(try_step(&mut w, Tick(1), &TickInput::default(), 50, StepReason::AuthorityFrame).unwrap_err().message.contains("budget"));
    println!("PASS work budget faults instead of yielding");
    let modules = BTreeMap::from([("main".into(), "#include helper; main(){ if(double(4) != 8) bad(); } bad(){return 1/0;}".into()), ("helper".into(), "double(n){ return n*2; }".into())]);
    let program = Program::load(&modules, &["main"], &Catalog::iw4()).unwrap();
    assert_eq!(program.modules().len(), 2);
    let mut w = SimWorld::new(); w.install_gsc_program(program, NativeRegistry::default()).unwrap();
    w.start_gsc("main::main", Value::Undefined, vec![]).unwrap(); tick(&mut w, 1);
    println!("PASS module imports resolved before execution");
    let mut w = world(r#"
        main() {
            level notify("past", 9);
            thread late(); thread wrong_receiver();
            wait 0;
            level notify("same", 10);
            wait 0;
            if(isdefined(level.bad)) fail();
        }
        late() { level waittill("past"); level.bad = 1; }
        wrong_receiver() { other = spawnstruct(); other waittill("same"); level.bad = 2; }
        fail() { return 1/0; }
    "#);
    w.start_gsc("probe::main", Value::Undefined, vec![]).unwrap();
    tick(&mut w, 1); tick(&mut w, 2); tick(&mut w, 3);
    println!("PASS late waiters do not consume old notifications; receivers are isolated");
    let malformed = BTreeMap::from([("bad".into(), "main(){ unknown(); }".into())]);
    assert!(Program::load(&malformed, &["bad"], &Catalog::iw4()).unwrap_err().message.contains("unknown builtin"));
    assert!(sim::gsc_ir::normalize_module("../escape").is_err());
    assert!(sim::gsc_ir::normalize_module("/absolute").is_err());
    assert_eq!(compile("main(){}").fingerprint(), compile("main(){}").fingerprint());
    assert_ne!(compile("main(){}").fingerprint(), compile("main(){wait 1;}").fingerprint());
    w.shutdown_game();
    assert!(w.gsc_program_fingerprint().is_none());
    assert!(try_step(&mut w, Tick(4), &TickInput::default(), 50, StepReason::AuthorityFrame).is_err());
    println!("PASS unknown natives, unsafe module names, program fingerprints, shutdown reset");

    let mut w = world(r#"
        main() {
            level.a["nested"][0] = 3;
            alias = level.a;
            alias["nested"][0] += 7;
            if (level.a["nested"][0] != 3 || alias["nested"][0] != 10) fail();
            obj = spawnstruct(); level.a["obj"] = obj; alias = level.a;
            alias["obj"].n = 4; if (level.a["obj"].n != 4) fail();
            local[2] = 6; local[9] = 2; local["2"] = 4;
            if (local.size != 3 || local[2] != 6) fail();
            local[9] = undefined;
            if (local.size != 2 || isdefined(local[9])) fail();
            if ([[::twice]](4) != 8) fail();
            level.cb = ::twice;
            if ([[level.cb]](5) != 10) fail();
            o = spawnstruct(); o.n = 5;
            if (o [[::method]](3) != 8) fail();
            o thread [[::worker]]();
            sum = 0;
            for (i = 0; i < 5; i++) {
                switch (i) {
                    case 1: continue;
                    case 2: sum += 2; break;
                    default: sum += 1;
                }
            }
            if (sum != 5) fail();
            switch ("a") { case "a": sum++; case "b": sum++; break; default: fail(); }
            if (sum != 7) fail();
            switch (90) { case 1: fail(); }
            for (;;) { break; }
            v = (1, 2, 3); if (v[1] != 2) fail();
            wait 0;
            if (o.n != 6) fail();
        }
        twice(n) { return n * 2; }
        method(n) { return self.n + n; }
        worker() { self.n++; }
        fail() { return 1/0; }
    "#);
    w.start_gsc("probe::main", Value::Undefined, vec![]).unwrap();
    tick(&mut w, 1); let mut cloned = w.clone(); tick(&mut w, 2); tick(&mut cloned, 2);
    println!("PASS arrays, nested autovivification, typed keys, deletion, value copies with shared objects, callbacks, receiver threads, for/switch/continue/fallthrough, clone");


    let mut w = world(r#"
        #define FACTOR 2
        LIMIT = 3 + FACTOR;
        main() {
            a[5] = 7; a["key"] = 9;
            sum = 0;
            foreach (k, v in a) { switch (k) { case "key": continue; } sum += v; }
            if (sum != 7 || LIMIT != 5) fail();
            copyarg(a); if (a[5] != 7) fail();
            if ((6 & 3) != 2 || (1 << 3) != 8) fail();
            if ((0 ? fail() : 4) != 4) fail();
            if ((1 ? 3 : fail()) != 3) fail();
            level.box = spawnstruct(); level.box.circular = level.box;
            level.box thread waiter();
            thread sender();
            waittillframeend;
            if (level.payload[5] != 7) fail();
            level.payload[5] = 20;
            wait 0;
            if (level.payload[5] != 20 || level.box.circular != level.box) fail();
        }
        copyarg(a) { a[5] = 100; }
        waiter() { self waittill("ready", data); data["key"] = 10; level.payload = data; }
        sender() { b[5] = 7; b["key"] = 9; level.box notify("ready", b); waittillframeend; if (b["key"] != 9) fail(); }
        fail() { return 1/0; }
    "#);
    w.start_gsc("probe::main", Value::Undefined, vec![]).unwrap(); tick(&mut w, 1);
    let mut cloned = w.clone(); tick(&mut w, 2); tick(&mut cloned, 2);
    println!("PASS foreach, constants, bit operators, ternary short circuit, argument and event array copies, indexed event receivers, end-of-frame scheduling, cyclic object roots and clone");
    let mut w = world(r#"
        main() { level.mover = self; self thread sleeper(); self delete(); fail(); }
        sleeper() { wait 0; fail(); }
        observer() { if (isdefined(level.mover)) fail(); wait 0; if (isdefined(level.mover)) fail(); }
        fail() { return 1/0; }
    "#);
    let number = w.spawn_script_mover(ScriptModelId::from_wire(18), [0.0;3], [0.0;3]).unwrap();
    let receiver = w.entity_kernel().current_ref(number).unwrap();
    w.start_gsc("probe::main", Value::Entity(receiver), vec![]).unwrap();
    w.start_gsc("probe::observer", Value::Undefined, vec![]).unwrap();
    tick(&mut w, 1); tick(&mut w, 2);
    assert!(w.script_mover_by_number(number).is_none());
    println!("PASS native mover deletion, immediate self-thread cancellation, cancellation of children and stale isdefined");
    let mut w = world("main() { a = []; a[-1] = 1; }");
    w.start_gsc("probe::main", Value::Undefined, vec![]).unwrap();
    assert!(try_step(&mut w, Tick(1), &TickInput::default(), 50, StepReason::AuthorityFrame).unwrap_err().message.contains("array index"));
    println!("PASS invalid array index rejects with a runtime fault");

    println!("--- V0 ---");
    run_logged("int/int divide", r#"main() {
        log(7 / 2, 6 / 3, 7.0 / 2, -7 / 2);
        if (7 / 2 != 3.5) fail();
        fail_if_not_float(6 / 3);
    }
    fail_if_not_float(v) { if (("" + v) != "2") fail(); }
    fail() { return 1/0; }"#, 1).unwrap();
    expect_fault("int/int divide by zero", "main() { x = 1 / 0; }", "divide by 0");
    run_logged("modulo", r#"main() { log(7 % 3, -7 % 3, 7 % -3, 1 << 33, ~5, 6 ^ 3); }"#, 1).unwrap();
    expect_fault("modulo by zero", "main() { x = 7 % 0; }", "divide by 0");
    expect_fault("modulo on float", "main() { x = 7.5 % 2; }", "requires int");
    run_logged("float to string concat", r#"main() {
        log("" + 1.5, "" + 1000000.0, "" + 123456.0, "" + 0.1, "" + 0.0001, "" + 0.00001);
        log("" + 7 / 2, "n" + 5, 2.5 + "s", "v" + (1, 2.5, -3), "" + 1.0 / 3);
        if ("" + 1000000.0 != "1e+006") fail();
        if ("" + 1.5 != "1.5") fail();
    }
    fail() { return 1/0; }"#, 1).unwrap();
    expect_fault("vector + scalar", "main() { x = (1, 2, 3) + 1; }", "unmatching types");
    run_logged("vector * scalar splat", r#"main() { log("" + (1, 2, 3) * 2, "" + 2 * (1, 2, 3), "" + (2, 4, 6) / 2); }"#, 1).unwrap();
    expect_fault("int == undefined", "main() { if (5 == undefined) x = 1; }", "unmatching types");
    expect_fault("truth of undefined", "main() { if (undefined) x = 1; }", "cast from undefined to bool");
    expect_fault("truth of string", r#"main() { if ("s") x = 1; }"#, "cast from string to bool");
    expect_fault("array equality", "main() { a = []; b = []; if (a == b) x = 1; }", "cannot compare arrays");
    run_logged("notify during notify", r#"main() {
        level.o = spawnstruct();
        thread w1(); thread w2(); thread w3(); thread w4();
        wait 0.05;
        log("main notify a");
        level notify("a");
        log("main after notify");
    }
    w1() { level waittill("a"); log("w1 resumed"); }
    w2() { level waittill("a"); log("w2 resumed, notifies b"); level notify("b"); log("w2 after notify b"); }
    w3() { level waittill("a"); log("w3 resumed"); }
    w4() { level waittill("b"); log("w4 resumed"); }"#, 2).unwrap();
    run_logged("endon vs waittill on the same event", r#"main() {
        thread root_waiter();
        thread caller();
        thread returned_endon();
        wait 0.05;
        log("main notify x");
        level notify("x");
        log("main notify y");
        level notify("y");
    }
    root_waiter() { level endon("x"); level waittill("x"); log("root_waiter: waittill won (wrong)"); }
    caller() { r = sub(); log("caller resumed after sub was ended", r); }
    sub() { level endon("x"); level waittill("x"); log("sub: waittill won (wrong)"); return 5; }
    returned_endon() { r = scoped(); log("scoped returned", r); level waittill("y"); log("returned_endon: y delivered, endon was frame scoped"); }
    scoped() { level endon("y"); return 1; }"#, 2).unwrap();
    run_logged("notifier ends its own frame", r#"main() { r = sub(); log("main resumed in caller", r); }
    sub() { level endon("z"); level notify("z"); log("sub continued (wrong)"); return 1; }"#, 1).unwrap();
    run_logged("waittillmatch", r#"main() {
        thread matcher();
        thread plain();
        wait 0.05;
        level notify("m", "stop"); log("sent stop");
        level notify("m"); log("sent no payload");
        level notify("m", 5); log("sent int 5");
        level notify("m", "go", 3); log("sent go, 3");
    }
    matcher() { level waittillmatch("m", "go"); log("matcher matched"); }
    plain() { level waittill("m", v); log("plain got", v); }"#, 2).unwrap();
    run_logged("long int literal wraps; native function reference", r#"main() {
        f = ::isdefined; g = ::isdefined;
        log(100000000000, 100000000001, -2147483648, [[f]](1), f == g);
    }"#, 1).unwrap();

    println!("--- V1 ---");
    run_logged("thread runs inline until its first yield", r#"main() {
        log("main before", gettime());
        thread child();
        log("main after", level.set);
        wait 0.05;
        log("main resumed", gettime());
    }
    child() { level.set = 1; log("child start"); wait 0; log("child after wait 0", gettime()); }"#, 2).unwrap();
    run_logged("endon fires on a suspended spawner", r#"main() { r = sub(); log("main got", r); }
    sub() { level endon("k"); thread killer(); log("sub continued (wrong)"); return 1; }
    killer() { level notify("k"); log("killer continues"); }"#, 1).unwrap();
    run_logged("wait quantization", r#"main() {
        foreach (w in array(0.01, 0.025, 0.07, 0.075, 0.08, 1)) { t = gettime(); wait w; log(w, gettime() - t); }
    }
    array(a, b, c, d, e, f) { r[0] = a; r[1] = b; r[2] = c; r[3] = d; r[4] = e; r[5] = f; return r; }"#, 60).unwrap();
    expect_fault("negative wait", "main() { wait -1; }", "negative wait");
    expect_fault("wait too long", "main() { wait 1000000; }", "wait is too long");
    expect_fault("call depth", "main() { f(0); } f(n) { return f(n + 1); }", "stack overflow");
    let fault = run_logged("thread nesting depth", r#"main() { thread t(0); }
    t(n) { level.deepest = n; if (n % 10 == 9) log(n); thread t(n + 1); }"#, 1).unwrap_err();
    println!("  fault: {fault}");
}
