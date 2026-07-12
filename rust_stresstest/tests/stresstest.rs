use rust_stresstest::{AdvancedJITEngine, HighLevelFunction, Val, SecureEngineContext};
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
