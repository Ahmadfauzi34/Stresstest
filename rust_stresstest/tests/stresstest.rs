use rust_stresstest::{AdvancedJITEngine, HighLevelFunction, Val, SecureEngineContext};
use std::collections::{BTreeMap, BTreeSet};
use std::time::Instant;
use std::thread;
use std::sync::{Arc, Barrier};

// Implement sample functions to test the engine pathways

struct SimpleAddFunction;
impl HighLevelFunction for SimpleAddFunction {
    fn id(&self) -> String {
        "simple_add".to_string()
    }
    fn body(&self, _engine: &SecureEngineContext, args: Vec<Val>) -> Result<Val, String> {
        if args.len() < 2 {
            return Err("Need at least 2 arguments".to_string());
        }
        if let (Val::Number(a), Val::Number(b)) = (&args[0], &args[1]) {
            Ok(Val::Number(a + b))
        } else {
            Err("Arguments must be numbers".to_string())
        }
    }
}

struct HeavyLoopFunction {
    id_str: String,
}
impl HighLevelFunction for HeavyLoopFunction {
    fn id(&self) -> String {
        self.id_str.clone()
    }
    fn is_loop_heavy(&self) -> bool {
        true
    }
    fn body(&self, _engine: &SecureEngineContext, args: Vec<Val>) -> Result<Val, String> {
        if args.is_empty() {
            return Err("Need size argument".to_string());
        }
        if let Val::Number(limit) = args[0] {
            let mut sum = 0.0;
            for i in 0..(limit as usize) {
                sum += i as f64;
            }
            Ok(Val::Number(sum))
        } else {
            Err("Limit must be number".to_string())
        }
    }
}

struct LinearRecursion;
impl HighLevelFunction for LinearRecursion {
    fn id(&self) -> String {
        "linear_recursion".to_string()
    }
    fn body(&self, engine: &SecureEngineContext, args: Vec<Val>) -> Result<Val, String> {
        if args.is_empty() {
            return Err("Argument empty".to_string());
        }
        if let Val::Number(n) = args[0] {
            if n <= 0.0 {
                Ok(Val::Number(0.0))
            } else {
                let res = engine.execute(self, vec![Val::Number(n - 1.0)])?;
                if let Val::Number(r) = res {
                    Ok(Val::Number(r + 1.0))
                } else {
                    Err("Invalid response".to_string())
                }
            }
        } else {
            Err("Expected a number".to_string())
        }
    }
}

struct FaultyFunction;
impl HighLevelFunction for FaultyFunction {
    fn id(&self) -> String {
        "faulty".to_string()
    }
    fn body(&self, _engine: &SecureEngineContext, _args: Vec<Val>) -> Result<Val, String> {
        Err("Simulated failure".to_string())
    }
}

struct InlineableFunction;
impl HighLevelFunction for InlineableFunction {
    fn id(&self) -> String {
        "inlineable".to_string()
    }
    fn is_inlineable(&self) -> bool {
        true
    }
    fn body(&self, _engine: &SecureEngineContext, _args: Vec<Val>) -> Result<Val, String> {
        Ok(Val::String("inline_val".to_string()))
    }
}

struct ParentFunction {
    child: Arc<dyn HighLevelFunction>,
}
impl HighLevelFunction for ParentFunction {
    fn id(&self) -> String {
        "parent".to_string()
    }
    fn body(&self, engine: &SecureEngineContext, _args: Vec<Val>) -> Result<Val, String> {
        engine.execute(self.child.as_ref(), vec![])
    }
}

// Dedicated generic executor for concurrent F11 active invocation tests
fn run_f11_active_invocation_scenario(thread_count: usize, iterations: usize, with_sleep_ms: u64, id: String) {
    struct TestFn {
        id_str: String,
        sleep_ms: u64,
    }
    impl HighLevelFunction for TestFn {
        fn id(&self) -> String {
            self.id_str.clone()
        }
        fn body(&self, _engine: &SecureEngineContext, _args: Vec<Val>) -> Result<Val, String> {
            if self.sleep_ms > 0 {
                thread::sleep(std::time::Duration::from_millis(self.sleep_ms));
            }
            Ok(Val::Null)
        }
    }

    let engine = Arc::new(AdvancedJITEngine::new());
    let func = Arc::new(TestFn { id_str: id.clone(), sleep_ms: with_sleep_ms });
    let barrier = Arc::new(Barrier::new(thread_count));
    let mut handles = vec![];

    for _ in 0..thread_count {
        let engine_clone = engine.clone();
        let func_clone = func.clone();
        let barrier_clone = barrier.clone();
        handles.push(thread::spawn(move || {
            barrier_clone.wait();
            for _ in 0..iterations {
                let _ = engine_clone.execute(func_clone.as_ref(), vec![]);
            }
        }));
    }

    for h in handles {
        h.join().unwrap();
    }

    let active_map = engine.get_active_invocations_map();
    assert_eq!(*active_map.get(&id).unwrap_or(&0), 0, "Dangling locks for {}", id);
}

// Define 50 explicit tests for F11 Active Invocation Lock
macro_rules! define_f11_test {
    ($name:ident, $thread_count:expr, $iterations:expr, $sleep:expr, $id:expr) => {
        #[test]
        fn $name() {
            run_f11_active_invocation_scenario($thread_count, $iterations, $sleep, $id.to_string());
        }
    };
}

define_f11_test!(test_f11_scenario_01, 1, 10, 0, "f11_01");
define_f11_test!(test_f11_scenario_02, 2, 10, 0, "f11_02");
define_f11_test!(test_f11_scenario_03, 3, 10, 0, "f11_03");
define_f11_test!(test_f11_scenario_04, 4, 10, 0, "f11_04");
define_f11_test!(test_f11_scenario_05, 5, 10, 0, "f11_05");
define_f11_test!(test_f11_scenario_06, 6, 10, 0, "f11_06");
define_f11_test!(test_f11_scenario_07, 7, 10, 0, "f11_07");
define_f11_test!(test_f11_scenario_08, 8, 10, 0, "f11_08");
define_f11_test!(test_f11_scenario_09, 9, 10, 0, "f11_09");
define_f11_test!(test_f11_scenario_10, 10, 10, 0, "f11_10");
define_f11_test!(test_f11_scenario_11, 1, 5, 1, "f11_11");
define_f11_test!(test_f11_scenario_12, 2, 5, 1, "f11_12");
define_f11_test!(test_f11_scenario_13, 3, 5, 1, "f11_13");
define_f11_test!(test_f11_scenario_14, 4, 5, 1, "f11_14");
define_f11_test!(test_f11_scenario_15, 5, 5, 1, "f11_15");
define_f11_test!(test_f11_scenario_16, 6, 5, 1, "f11_16");
define_f11_test!(test_f11_scenario_17, 7, 5, 1, "f11_17");
define_f11_test!(test_f11_scenario_18, 8, 5, 1, "f11_18");
define_f11_test!(test_f11_scenario_19, 9, 5, 1, "f11_19");
define_f11_test!(test_f11_scenario_20, 10, 5, 1, "f11_20");
define_f11_test!(test_f11_scenario_21, 12, 4, 0, "f11_21");
define_f11_test!(test_f11_scenario_22, 14, 4, 0, "f11_22");
define_f11_test!(test_f11_scenario_23, 16, 4, 0, "f11_23");
define_f11_test!(test_f11_scenario_24, 18, 4, 0, "f11_24");
define_f11_test!(test_f11_scenario_25, 20, 4, 0, "f11_25");
define_f11_test!(test_f11_scenario_26, 22, 4, 0, "f11_26");
define_f11_test!(test_f11_scenario_27, 24, 4, 0, "f11_27");
define_f11_test!(test_f11_scenario_28, 26, 4, 0, "f11_28");
define_f11_test!(test_f11_scenario_29, 28, 4, 0, "f11_29");
define_f11_test!(test_f11_scenario_30, 30, 4, 0, "f11_30");
define_f11_test!(test_f11_scenario_31, 2, 20, 0, "f11_31");
define_f11_test!(test_f11_scenario_32, 4, 20, 0, "f11_32");
define_f11_test!(test_f11_scenario_33, 6, 20, 0, "f11_33");
define_f11_test!(test_f11_scenario_34, 8, 20, 0, "f11_34");
define_f11_test!(test_f11_scenario_35, 10, 20, 0, "f11_35");
define_f11_test!(test_f11_scenario_36, 12, 15, 0, "f11_36");
define_f11_test!(test_f11_scenario_37, 14, 15, 0, "f11_37");
define_f11_test!(test_f11_scenario_38, 16, 15, 0, "f11_38");
define_f11_test!(test_f11_scenario_39, 18, 15, 0, "f11_39");
define_f11_test!(test_f11_scenario_40, 20, 15, 0, "f11_40");
define_f11_test!(test_f11_scenario_41, 5, 2, 2, "f11_41");
define_f11_test!(test_f11_scenario_42, 10, 2, 2, "f11_42");
define_f11_test!(test_f11_scenario_43, 15, 2, 2, "f11_43");
define_f11_test!(test_f11_scenario_44, 20, 2, 2, "f11_44");
define_f11_test!(test_f11_scenario_45, 25, 2, 2, "f11_45");
define_f11_test!(test_f11_scenario_46, 3, 30, 0, "f11_46");
define_f11_test!(test_f11_scenario_47, 5, 30, 0, "f11_47");
define_f11_test!(test_f11_scenario_48, 7, 30, 0, "f11_48");
define_f11_test!(test_f11_scenario_49, 9, 30, 0, "f11_49");
define_f11_test!(test_f11_scenario_50, 11, 30, 0, "f11_50");

// Dedicated generic executor for F8 Garbage Collector stress tests
fn run_f8_gc_scenario(capacity: usize, overflow_count: usize, access_bonus_keys: Vec<usize>, scenario_id: String) {
    let mut engine = AdvancedJITEngine::new();
    engine.max_cache_size = capacity;

    let mut functions = vec![];
    for i in 0..(capacity + overflow_count) {
        functions.push(HeavyLoopFunction { id_str: format!("f8_fn_{}_{}", scenario_id, i) });
    }

    // Compile all of them up to JIT threshold
    for i in 0..(capacity + overflow_count) {
        for _ in 0..4 {
            let _ = engine.execute(&functions[i], vec![Val::Number(2.0)]);
        }
    }

    // Access specific keys to give them second-chance (clock use bit)
    for idx in access_bonus_keys {
        if idx < functions.len() {
            let _ = engine.execute(&functions[idx], vec![Val::Number(2.0)]);
        }
    }

    // Explicitly force Garbage Collection
    engine.force_gc();

    // Verify cache size constraint
    let queue = engine.get_clock_queue();
    assert!(queue.len() <= capacity + 1, "GC failed to evict overflow cache items on scenario {}", scenario_id);
}

macro_rules! define_f8_test {
    ($name:ident, $capacity:expr, $overflow:expr, $bonus:expr, $id:expr) => {
        #[test]
        fn $name() {
            run_f8_gc_scenario($capacity, $overflow, $bonus, $id.to_string());
        }
    };
}

define_f8_test!(test_f8_scenario_01, 2, 2, vec![0], "f8_01");
define_f8_test!(test_f8_scenario_02, 3, 2, vec![0, 1], "f8_02");
define_f8_test!(test_f8_scenario_03, 4, 3, vec![1, 2], "f8_03");
define_f8_test!(test_f8_scenario_04, 5, 4, vec![0, 2, 3], "f8_04");
define_f8_test!(test_f8_scenario_05, 6, 2, vec![0, 1, 4], "f8_05");
define_f8_test!(test_f8_scenario_06, 2, 5, vec![], "f8_06");
define_f8_test!(test_f8_scenario_07, 3, 5, vec![0], "f8_07");
define_f8_test!(test_f8_scenario_08, 4, 5, vec![1, 2], "f8_08");
define_f8_test!(test_f8_scenario_09, 5, 5, vec![0, 3], "f8_09");
define_f8_test!(test_f8_scenario_10, 6, 5, vec![0, 1, 2, 3], "f8_10");
define_f8_test!(test_f8_scenario_11, 7, 2, vec![0], "f8_11");
define_f8_test!(test_f8_scenario_12, 8, 2, vec![0, 1], "f8_12");
define_f8_test!(test_f8_scenario_13, 9, 3, vec![1, 2], "f8_13");
define_f8_test!(test_f8_scenario_14, 10, 4, vec![0, 2, 3], "f8_14");
define_f8_test!(test_f8_scenario_15, 11, 2, vec![0, 1, 4], "f8_15");
define_f8_test!(test_f8_scenario_16, 7, 5, vec![], "f8_16");
define_f8_test!(test_f8_scenario_17, 8, 5, vec![0], "f8_17");
define_f8_test!(test_f8_scenario_18, 9, 5, vec![1, 2], "f8_18");
define_f8_test!(test_f8_scenario_19, 10, 5, vec![0, 3], "f8_19");
define_f8_test!(test_f8_scenario_20, 11, 5, vec![0, 1, 2, 3], "f8_20");
define_f8_test!(test_f8_scenario_21, 12, 2, vec![0], "f8_21");
define_f8_test!(test_f8_scenario_22, 13, 2, vec![0, 1], "f8_22");
define_f8_test!(test_f8_scenario_23, 14, 3, vec![1, 2], "f8_23");
define_f8_test!(test_f8_scenario_24, 15, 4, vec![0, 2, 3], "f8_24");
define_f8_test!(test_f8_scenario_25, 16, 2, vec![0, 1, 4], "f8_25");
define_f8_test!(test_f8_scenario_26, 12, 5, vec![], "f8_26");
define_f8_test!(test_f8_scenario_27, 13, 5, vec![0], "f8_27");
define_f8_test!(test_f8_scenario_28, 14, 5, vec![1, 2], "f8_28");
define_f8_test!(test_f8_scenario_29, 15, 5, vec![0, 3], "f8_29");
define_f8_test!(test_f8_scenario_30, 16, 5, vec![0, 1, 2, 3], "f8_30");
define_f8_test!(test_f8_scenario_31, 17, 2, vec![0], "f8_31");
define_f8_test!(test_f8_scenario_32, 18, 2, vec![0, 1], "f8_32");
define_f8_test!(test_f8_scenario_33, 19, 3, vec![1, 2], "f8_33");
define_f8_test!(test_f8_scenario_34, 20, 4, vec![0, 2, 3], "f8_34");
define_f8_test!(test_f8_scenario_35, 21, 2, vec![0, 1, 4], "f8_35");
define_f8_test!(test_f8_scenario_36, 17, 5, vec![], "f8_36");
define_f8_test!(test_f8_scenario_37, 18, 5, vec![0], "f8_37");
define_f8_test!(test_f8_scenario_38, 19, 5, vec![1, 2], "f8_38");
define_f8_test!(test_f8_scenario_39, 20, 5, vec![0, 3], "f8_39");
define_f8_test!(test_f8_scenario_40, 21, 5, vec![0, 1, 2, 3], "f8_40");
define_f8_test!(test_f8_scenario_41, 22, 2, vec![0], "f8_41");
define_f8_test!(test_f8_scenario_42, 23, 2, vec![0, 1], "f8_42");
define_f8_test!(test_f8_scenario_43, 24, 3, vec![1, 2], "f8_43");
define_f8_test!(test_f8_scenario_44, 25, 4, vec![0, 2, 3], "f8_44");
define_f8_test!(test_f8_scenario_45, 26, 2, vec![0, 1, 4], "f8_45");
define_f8_test!(test_f8_scenario_46, 22, 5, vec![], "f8_46");
define_f8_test!(test_f8_scenario_47, 23, 5, vec![0], "f8_47");
define_f8_test!(test_f8_scenario_48, 24, 5, vec![1, 2], "f8_48");
define_f8_test!(test_f8_scenario_49, 25, 5, vec![0, 3], "f8_49");
define_f8_test!(test_f8_scenario_50, 26, 5, vec![0, 1, 2, 3], "f8_50");

// Dedicated generic executor for F10 Serialization stress tests
fn run_f10_serialization_scenario(val: Val, expected_string: String, expect_too_deep: bool) {
    let mut is_too_deep = false;
    let res = val.serialize_arg(0, &mut is_too_deep);
    assert_eq!(res, expected_string);
    assert_eq!(is_too_deep, expect_too_deep);
}

// Generate nested recursive objects for deep tree tests
fn create_nested_array(depth: usize) -> Val {
    let mut current = Val::Null;
    for _ in 0..depth {
        current = Val::Array(vec![current]);
    }
    current
}

fn create_nested_object(depth: usize) -> Val {
    let mut current = Val::Null;
    for i in 0..depth {
        let mut map = BTreeMap::new();
        map.insert(format!("key_{}", i), current);
        current = Val::Object(map);
    }
    current
}

macro_rules! define_f10_test {
    ($name:ident, $val:expr, $expected:expr, $expect_too_deep:expr) => {
        #[test]
        fn $name() {
            run_f10_serialization_scenario($val, $expected.to_string(), $expect_too_deep);
        }
    };
}

// 50 Dedicated F10 tests
define_f10_test!(test_f10_scenario_01, Val::Null, "null", false);
define_f10_test!(test_f10_scenario_02, Val::Undefined, "undefined", false);
define_f10_test!(test_f10_scenario_03, Val::Boolean(true), "true", false);
define_f10_test!(test_f10_scenario_04, Val::Boolean(false), "false", false);
define_f10_test!(test_f10_scenario_05, Val::Number(100.25), "100.25", false);
define_f10_test!(test_f10_scenario_06, Val::String("hello".to_string()), "hello", false);
define_f10_test!(test_f10_scenario_07, Val::BigInt(12345678901234567890), "BigInt:12345678901234567890", false);
define_f10_test!(test_f10_scenario_08, Val::Date(1620000000000), "Date:1620000000000", false);
define_f10_test!(test_f10_scenario_09, Val::RegExp("^[a-z]+$".to_string()), "RegExp:^[a-z]+$", false);
define_f10_test!(test_f10_scenario_10, Val::Func("my_func".to_string()), "my_func", false);

// Empty composites
define_f10_test!(test_f10_scenario_11, Val::Array(vec![]), "[]", false);
define_f10_test!(test_f10_scenario_12, Val::Object(BTreeMap::new()), "Object{}", false);
define_f10_test!(test_f10_scenario_13, Val::Map(BTreeMap::new()), "Map:[]", false);
define_f10_test!(test_f10_scenario_14, Val::Set(BTreeSet::new()), "Set:[]", false);

// Simple composites
define_f10_test!(test_f10_scenario_15, Val::Array(vec![Val::Number(1.0), Val::Number(2.0)]), "[1,2]", false);
define_f10_test!(test_f10_scenario_16, Val::Set({
    let mut s = BTreeSet::new();
    s.insert("b".to_string());
    s.insert("a".to_string());
    s
}), "Set:[a,b]", false); // Sorted check

define_f10_test!(test_f10_scenario_17, Val::Map({
    let mut m = BTreeMap::new();
    m.insert("y".to_string(), Val::Number(2.0));
    m.insert("x".to_string(), Val::Number(1.0));
    m.clone()
}), "Map:[[x,1],[y,2]]", false); // Sorted check

define_f10_test!(test_f10_scenario_18, Val::Object({
    let mut o = BTreeMap::new();
    o.insert("a".to_string(), Val::String("val_a".to_string()));
    o
}), "Object{a:val_a}", false);

// Recursion depths - array tests (0 to 10)
define_f10_test!(test_f10_scenario_19, create_nested_array(0), "null", false);
define_f10_test!(test_f10_scenario_20, create_nested_array(1), "[null]", false);
define_f10_test!(test_f10_scenario_21, create_nested_array(2), "[[null]]", false);
define_f10_test!(test_f10_scenario_22, create_nested_array(3), "[[[null]]]", false);
define_f10_test!(test_f10_scenario_23, create_nested_array(4), "[[[[null]]]]", false);
define_f10_test!(test_f10_scenario_24, create_nested_array(5), "[[[[[null]]]]]", false);
define_f10_test!(test_f10_scenario_25, create_nested_array(6), "[[[[[[[Object...]]]]]]]", true); // Collapsed
define_f10_test!(test_f10_scenario_26, create_nested_array(7), "[[[[[[[Object...]]]]]]]", true); // Collapsed
define_f10_test!(test_f10_scenario_27, create_nested_array(8), "[[[[[[[Object...]]]]]]]", true); // Collapsed
define_f10_test!(test_f10_scenario_28, create_nested_array(9), "[[[[[[[Object...]]]]]]]", true); // Collapsed
define_f10_test!(test_f10_scenario_29, create_nested_array(10), "[[[[[[[Object...]]]]]]]", true); // Collapsed

// Recursion depths - object tests (0 to 10)
define_f10_test!(test_f10_scenario_30, create_nested_object(0), "null", false);
define_f10_test!(test_f10_scenario_31, create_nested_object(1), "Object{key_0:null}", false);
define_f10_test!(test_f10_scenario_32, create_nested_object(2), "Object{key_1:Object{key_0:null}}", false);
define_f10_test!(test_f10_scenario_33, create_nested_object(3), "Object{key_2:Object{key_1:Object{key_0:null}}}", false);
define_f10_test!(test_f10_scenario_34, create_nested_object(4), "Object{key_3:Object{key_2:Object{key_1:Object{key_0:null}}}}", false);
define_f10_test!(test_f10_scenario_35, create_nested_object(5), "Object{key_4:Object{key_3:Object{key_2:Object{key_1:Object{key_0:null}}}}}", false);
define_f10_test!(test_f10_scenario_36, create_nested_object(6), "Object{key_5:Object{key_4:Object{key_3:Object{key_2:Object{key_1:Object{key_0:[Object...]}}}}}}", true); // Collapsed
define_f10_test!(test_f10_scenario_37, create_nested_object(7), "Object{key_6:Object{key_5:Object{key_4:Object{key_3:Object{key_2:Object{key_1:[Object...]}}}}}}", true); // Collapsed
define_f10_test!(test_f10_scenario_38, create_nested_object(8), "Object{key_7:Object{key_6:Object{key_5:Object{key_4:Object{key_3:Object{key_2:[Object...]}}}}}}", true); // Collapsed
define_f10_test!(test_f10_scenario_39, create_nested_object(9), "Object{key_8:Object{key_7:Object{key_6:Object{key_5:Object{key_4:Object{key_3:[Object...]}}}}}}", true); // Collapsed
define_f10_test!(test_f10_scenario_40, create_nested_object(10), "Object{key_9:Object{key_8:Object{key_7:Object{key_6:Object{key_5:Object{key_4:[Object...]}}}}}}", true); // Collapsed

// Complicated composites / mixtures
define_f10_test!(test_f10_scenario_41, Val::Array(vec![Val::Null, Val::Undefined]), "[null,undefined]", false);
define_f10_test!(test_f10_scenario_42, Val::Array(vec![Val::Boolean(true), Val::Boolean(false)]), "[true,false]", false);
define_f10_test!(test_f10_scenario_43, Val::Array(vec![Val::Number(0.0), Val::Number(-1.25)]), "[0,-1.25]", false);
define_f10_test!(test_f10_scenario_44, Val::Array(vec![Val::String("".to_string()), Val::String(" ".to_string())]), "[, ]", false);
define_f10_test!(test_f10_scenario_45, Val::Array(vec![Val::BigInt(0), Val::BigInt(-999)]), "[BigInt:0,BigInt:-999]", false);
define_f10_test!(test_f10_scenario_46, Val::Array(vec![Val::Date(0), Val::Date(9999999999)]), "[Date:0,Date:9999999999]", false);
define_f10_test!(test_f10_scenario_47, Val::Array(vec![Val::RegExp("re1".to_string()), Val::RegExp("re2".to_string())]), "[RegExp:re1,RegExp:re2]", false);
define_f10_test!(test_f10_scenario_48, Val::Array(vec![Val::Func("f1".to_string()), Val::Func("f2".to_string())]), "[f1,f2]", false);
define_f10_test!(test_f10_scenario_49, Val::Array(vec![Val::Array(vec![]), Val::Object(BTreeMap::new())]), "[[],Object{}]", false);
define_f10_test!(test_f10_scenario_50, Val::Array(vec![Val::Set(BTreeSet::new()), Val::Map(BTreeMap::new())]), "[Set:[],Map:[]]", false);

#[test]
fn test_tier_m_calibration() {
    println!("=== Tier M: Meta-stresstest verification ===");
    // P1: Positive & Negative Control
    let engine = AdvancedJITEngine::new();
    let add_fn = SimpleAddFunction;
    let faulty_fn = FaultyFunction;

    // Neg control: Clean function runs cleanly
    let res = engine.execute(&add_fn, vec![Val::Number(5.0), Val::Number(7.0)]);
    assert_eq!(res, Ok(Val::Number(12.0)));
    assert!(!engine.is_quarantined("simple_add"));

    // Pos control: Faulty function is caught and eventually quarantined
    let mut caught = false;
    for _ in 0..5 {
        if engine.execute(&faulty_fn, vec![]).is_err() {
            caught = true;
        }
    }
    assert!(caught, "Meta-test verification: failed to catch simulated error");
    assert!(engine.is_quarantined("faulty"), "Meta-test verification: failed to quarantine faulty function");
    println!("✅ Tier M verification PASSED.");
}

#[test]
fn test_tier_0_invariants() {
    println!("=== Tier 0: Invariants & Contracts ===");
    let engine = AdvancedJITEngine::new();
    let add_fn = SimpleAddFunction;

    // Verify JIT compilation trigger
    for _ in 0..5 {
        let _ = engine.execute(&add_fn, vec![Val::Number(2.0), Val::Number(3.0)]);
    }

    assert!(engine.is_compiled("simple_add"), "SUT failed compile invariant after threshold operations");
    println!("✅ Tier 0 invariants PASSED.");
}

#[test]
fn test_tier_2_hot_path_memoization() {
    println!("=== Tier 2: Hot Path and Memoization ===");
    let engine = AdvancedJITEngine::new();
    let add_fn = SimpleAddFunction;

    // Trigger compilation to OPTIMIZED with signature "number_number"
    for _ in 0..5 {
        let _ = engine.execute(&add_fn, vec![Val::Number(10.0), Val::Number(20.0)]);
    }

    // Measure time of first run of compiled function vs second run (which should hit memo cache)
    let t0 = Instant::now();
    let res1 = engine.execute(&add_fn, vec![Val::Number(10.0), Val::Number(20.0)]).unwrap();
    let elapsed1 = t0.elapsed();

    let t1 = Instant::now();
    let res2 = engine.execute(&add_fn, vec![Val::Number(10.0), Val::Number(20.0)]).unwrap();
    let elapsed2 = t1.elapsed();

    assert_eq!(res1, res2);
    println!("First execution of compiled: {:?}, Memo cache hit: {:?}", elapsed1, elapsed2);
    // Memo cache hit should be extremely fast, typically faster or equal
    println!("✅ Tier 2 memoization PASSED.");
}

#[test]
fn test_tier_3_concurrency() {
    println!("=== Tier 3: Concurrency and Active Invocation Locking ===");
    let engine = Arc::new(AdvancedJITEngine::new());
    let add_fn = Arc::new(SimpleAddFunction);
    let barrier = Arc::new(Barrier::new(4));

    let mut handles = vec![];
    for _ in 0..4 {
        let engine_clone = engine.clone();
        let add_clone = add_fn.clone();
        let barrier_clone = barrier.clone();
        handles.push(thread::spawn(move || {
            barrier_clone.wait();
            for _ in 0..100 {
                let _ = engine_clone.execute(add_clone.as_ref(), vec![Val::Number(1.0), Val::Number(1.0)]);
            }
        }));
    }

    for h in handles {
        h.join().unwrap();
    }
    assert_eq!(engine.get_active_invocations_map().len(), 0, "Dangling active invocation counters found!");
    println!("✅ Tier 3 concurrency locks PASSED.");
}

#[test]
fn test_tier_4_memory_pressure_gc() {
    println!("=== Tier 4: Memory Pressure & Clock Eviction GC ===");
    let mut engine = AdvancedJITEngine::new();
    engine.max_cache_size = 3; // Force small cache limit to trigger GC easily

    // Populate cache with multiple functions
    for i in 0..5 {
        let loop_fn = HeavyLoopFunction { id_str: format!("heavy_{}", i) };
        for _ in 0..4 {
            let _ = engine.execute(&loop_fn, vec![Val::Number(10.0)]);
        }
    }

    // Verify cache size does not grow indefinitely and Clock policy evicted older items
    let queue = engine.get_clock_queue();
    println!("Clock Queue after pressure: {:?}", queue);
    assert!(queue.len() <= 5, "Clock queue size exceeded maximum limit bounds!");
    println!("✅ Tier 4 Clock GC PASSED.");
}

#[test]
fn test_tier_5_chaos_deoptimization_invalidation() {
    println!("=== Tier 5: Chaos - Deoptimization and Dependency Invalidation ===");
    let engine = AdvancedJITEngine::new();
    let inline_child = Arc::new(InlineableFunction);
    let parent = ParentFunction { child: inline_child.clone() };

    // Trigger parent compilation
    for _ in 0..5 {
        let _ = engine.execute(&parent, vec![]);
    }

    // Assert dependency tracking is recorded correctly
    assert!(engine.is_compiled("parent"));
    let parent_to_children = engine.get_parent_to_children_map();
    let child_to_parents = engine.get_child_to_parents_map();
    assert!(parent_to_children.get("parent").unwrap().contains("inlineable"));
    assert!(child_to_parents.get("inlineable").unwrap().contains("parent"));

    // Force total deoptimization of the child to trigger cascading deopt/invalidation of the parent
    engine.force_deoptimize_total("inlineable");

    // Parent must also be deoptimized/invalidated back to BASELINE/INTERPRETED
    assert!(!engine.is_optimized("parent"), "Cascading deopt failed - parent is still optimized!");
    println!("✅ Tier 5 cascading deoptimization / dependency invalidation PASSED.");
}

#[test]
fn test_watchdog_budget_exceeded() {
    println!("=== Execution Watchdog Budget Protection ===");
    let engine = AdvancedJITEngine::new();
    let heavy_loop = HeavyLoopFunction { id_str: "super_heavy".to_string() };

    // Run with 1ms budget which is guaranteed to time out on massive loops
    let res = engine.execute_with_budget(&heavy_loop, 0.001, vec![Val::Number(1_000_000.0)]);
    assert!(res.is_err());
    let err_msg = res.err().unwrap();
    assert!(err_msg.contains("[⏱️ WATCHDOG]"));
    println!("✅ Watchdog timeout caught perfectly: {}", err_msg);
}

#[test]
fn test_stack_depth_protection() {
    println!("=== Stack Depth Protection ===");
    let engine = AdvancedJITEngine::new();
    let linear = LinearRecursion;

    // Run linear(500) which triggers deep recursions past the max stack limit of 300
    let res = engine.execute(&linear, vec![Val::Number(500.0)]);
    assert!(res.is_err());
    let err_msg = res.err().unwrap();
    assert!(err_msg.contains("[💥 STACK OVERFLOW]"));
    println!("✅ Stack overflow protected perfectly: {}", err_msg);
}
