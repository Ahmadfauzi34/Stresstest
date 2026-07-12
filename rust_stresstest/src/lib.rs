use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};
use std::time::{Instant, Duration};
use std::sync::{Arc, Mutex};

#[derive(Debug, Clone, PartialEq)]
pub enum Val {
    Null,
    Undefined,
    Boolean(bool),
    Number(f64),
    String(String),
    BigInt(i128),
    Array(Vec<Val>),
    Object(BTreeMap<String, Val>),
    Map(BTreeMap<String, Val>),
    Set(BTreeSet<String>),
    Date(u64), // timestamp in ms
    RegExp(String),
    Func(String), // function represented by its name/id
}

impl Val {
    pub fn get_strict_type(&self) -> &'static str {
        match self {
            Val::Null => "null",
            Val::Undefined => "undefined",
            Val::Boolean(_) => "boolean",
            Val::Number(_) => "number",
            Val::String(_) => "string",
            Val::BigInt(_) => "bigint",
            Val::Array(_) => "array",
            Val::Func(_) => "function",
            _ => "object",
        }
    }

    pub fn serialize_arg(&self, depth: usize, is_too_deep: &mut bool) -> String {
        if depth > 5 {
            *is_too_deep = true;
            return "[Object...]".to_string();
        }
        match self {
            Val::Null => "null".to_string(),
            Val::Undefined => "undefined".to_string(),
            Val::Boolean(b) => b.to_string(),
            Val::Number(n) => n.to_string(),
            Val::String(s) => s.clone(),
            Val::BigInt(bi) => format!("BigInt:{}", bi),
            Val::Date(timestamp) => format!("Date:{}", timestamp),
            Val::RegExp(re) => format!("RegExp:{}", re),
            Val::Func(f) => f.clone(),
            Val::Array(arr) => {
                let serialized_items: Vec<String> = arr.iter().map(|item| item.serialize_arg(depth + 1, is_too_deep)).collect();
                format!("[{}]", serialized_items.join(","))
            }
            Val::Map(map) => {
                let mut entries = Vec::new();
                for (k, v) in map {
                    entries.push(Val::Array(vec![Val::String(k.clone()), v.clone()]));
                }
                entries.sort_by(|a, b| {
                    if let (Val::Array(av), Val::Array(bv)) = (a, b) {
                        if let (Val::String(ak), Val::String(bk)) = (&av[0], &bv[0]) {
                            ak.cmp(bk)
                        } else {
                            std::cmp::Ordering::Equal
                        }
                    } else {
                        std::cmp::Ordering::Equal
                    }
                });
                format!("Map:{}", Val::Array(entries).serialize_arg(depth + 1, is_too_deep))
            }
            Val::Set(set) => {
                let mut items: Vec<String> = set.iter().cloned().collect();
                items.sort();
                let val_items: Vec<Val> = items.into_iter().map(Val::String).collect();
                format!("Set:{}", Val::Array(val_items).serialize_arg(depth + 1, is_too_deep))
            }
            Val::Object(obj) => {
                let mut properties = Vec::new();
                for (k, v) in obj {
                    properties.push(format!("{}:{}", k, v.serialize_arg(depth + 1, is_too_deep)));
                }
                format!("Object{{{}}}", properties.join(","))
            }
        }
    }
}

pub fn get_strict_signature(args: &[Val]) -> String {
    let len = args.len();
    if len == 0 {
        "void".to_string()
    } else if len == 1 {
        args[0].get_strict_type().to_string()
    } else if len == 2 {
        format!("{}_{}", args[0].get_strict_type(), args[1].get_strict_type())
    } else {
        args.iter()
            .map(|arg| arg.get_strict_type())
            .collect::<Vec<_>>()
            .join("_")
    }
}

pub trait HighLevelFunction: Send + Sync {
    fn id(&self) -> String;
    fn body(&self, engine: &SecureEngineContext, args: Vec<Val>) -> Result<Val, String>;
    fn is_loop_heavy(&self) -> bool { false }
    fn is_inlineable(&self) -> bool { false }
}

pub struct SecureEngineContext {
    pub execute_fn: Arc<dyn Fn(&dyn HighLevelFunction, Vec<Val>) -> Result<Val, String> + Send + Sync>,
}

impl SecureEngineContext {
    pub fn execute(&self, fn_obj: &dyn HighLevelFunction, args: Vec<Val>) -> Result<Val, String> {
        (self.execute_fn)(fn_obj, args)
    }
}

#[derive(Clone)]
pub struct ExecutionBudget {
    pub deadline: Instant,
    pub remaining_ms: f64,
}

pub struct OptimizedVariant {
    pub expected_signature: String,
    pub memo_cache: BTreeMap<String, Val>,
}

pub struct BaselineVariant {
    pub last_used: Instant,
    pub access_count: usize,
}

pub enum Tier {
    OPTIMIZED,
    BASELINE,
}

pub struct CompiledCodePolymorphic {
    pub id: String,
    pub tier: Tier,
    pub variants: HashMap<String, OptimizedVariant>,
    pub baseline: Option<BaselineVariant>,
    pub last_used: Instant,
    pub access_count: usize,
}

pub struct CompiledOSR {
    pub id: String,
    pub last_used: Instant,
    pub access_count: usize,
}

// Struct to hold non-Sync / thread-unsafe engine fields inside a thread-safe wrapper
struct EngineState {
    polymorphic_counters: HashMap<String, HashMap<String, usize>>,
    global_invocation_counters: HashMap<String, usize>,
    blacklisted_polymorphic_functions: HashSet<String>,
    deopt_counters: HashMap<String, usize>,
    loop_counters: HashMap<String, usize>,
    compiled_cache: HashMap<String, CompiledCodePolymorphic>,
    osr_cache: HashMap<String, CompiledOSR>,
    error_counters: HashMap<String, usize>,
    quarantined_functions: HashSet<String>,
    parent_to_children: HashMap<String, HashSet<String>>,
    child_to_parents: HashMap<String, HashSet<String>>,
    active_invocations: HashMap<String, usize>,
    max_memory_mb: f64,
    clock_queue: Vec<String>,
    clock_hand: usize,
    clock_use_bits: HashMap<String, bool>,
}

#[derive(Clone)]
pub struct AdvancedJITEngine {
    state: Arc<Mutex<EngineState>>,
    pub max_cache_size: usize,
    pub jit_threshold: usize,
    pub osr_threshold: usize,
    pub max_error_threshold: usize,
    pub max_polymorphic_slots: usize,
    pub max_memo_cache_size: usize,
    pub max_global_slow_path_attempts: usize,
    pub max_deopt_threshold: usize,
    pub max_stack_depth: usize,
    pub default_time_budget_ms: f64,
}

impl AdvancedJITEngine {
    pub fn new() -> Self {
        println!("[SYSTEM] Advanced JIT Engine (Rust Hardened) Aktif.");
        Self {
            state: Arc::new(Mutex::new(EngineState {
                polymorphic_counters: HashMap::new(),
                global_invocation_counters: HashMap::new(),
                blacklisted_polymorphic_functions: HashSet::new(),
                deopt_counters: HashMap::new(),
                loop_counters: HashMap::new(),
                compiled_cache: HashMap::new(),
                osr_cache: HashMap::new(),
                error_counters: HashMap::new(),
                quarantined_functions: HashSet::new(),
                parent_to_children: HashMap::new(),
                child_to_parents: HashMap::new(),
                active_invocations: HashMap::new(),
                max_memory_mb: 50.0,
                clock_queue: Vec::new(),
                clock_hand: 0,
                clock_use_bits: HashMap::new(),
            })),
            max_cache_size: 10,
            jit_threshold: 3,
            osr_threshold: 3,
            max_error_threshold: 2,
            max_polymorphic_slots: 2,
            max_memo_cache_size: 5,
            max_global_slow_path_attempts: 10,
            max_deopt_threshold: 2,
            max_stack_depth: 300,
            default_time_budget_ms: 100.0,
        }
    }

    pub fn set_max_memory_mb(&self, val: f64) {
        self.state.lock().unwrap().max_memory_mb = val;
    }

    pub fn get_max_memory_mb(&self) -> f64 {
        self.state.lock().unwrap().max_memory_mb
    }

    pub fn get_clock_queue(&self) -> Vec<String> {
        self.state.lock().unwrap().clock_queue.clone()
    }

    pub fn get_clock_hand(&self) -> usize {
        self.state.lock().unwrap().clock_hand
    }

    pub fn get_clock_use_bits(&self) -> HashMap<String, bool> {
        self.state.lock().unwrap().clock_use_bits.clone()
    }

    pub fn get_memory_usage_mb(&self) -> f64 {
        let state = self.state.lock().unwrap();
        let mut total_bytes = 0.0;
        for (_id, compiled) in &state.compiled_cache {
            total_bytes += 1024.0 * 1024.0 * 1.5;
            if compiled.baseline.is_some() {
                total_bytes += 1024.0 * 1024.0 * 1.0;
            }
            for (_sig, variant) in &compiled.variants {
                total_bytes += 1024.0 * 1024.0 * 2.0;
                for (key, _val) in &variant.memo_cache {
                    total_bytes += key.len() as f64 * 2.0;
                    total_bytes += 100.0 * 2.0; // Approximation of value serialization length
                    total_bytes += 1024.0 * 256.0; // 0.25MB overhead per memo item
                }
            }
        }
        for (_id, _osr) in &state.osr_cache {
            total_bytes += 1024.0 * 1024.0 * 3.0;
        }
        total_bytes / (1024.0 * 1024.0)
    }

    pub fn is_quarantined(&self, fn_id: &str) -> bool {
        self.state.lock().unwrap().quarantined_functions.contains(fn_id)
    }

    pub fn is_compiled(&self, fn_id: &str) -> bool {
        self.state.lock().unwrap().compiled_cache.contains_key(fn_id)
    }

    pub fn is_optimized(&self, fn_id: &str) -> bool {
        let state = self.state.lock().unwrap();
        if let Some(compiled) = state.compiled_cache.get(fn_id) {
            matches!(compiled.tier, Tier::OPTIMIZED)
        } else {
            false
        }
    }

    pub fn is_osr_compiled(&self, fn_id: &str) -> bool {
        self.state.lock().unwrap().osr_cache.contains_key(fn_id)
    }

    fn add_to_clock_queue(&self, key: String) {
        let mut state = self.state.lock().unwrap();
        if !state.clock_queue.contains(&key) {
            state.clock_queue.push(key.clone());
        }
        state.clock_use_bits.insert(key, true);
    }

    fn clean_inlining_dependencies(&self, fn_id: &str) {
        let mut state = self.state.lock().unwrap();
        if let Some(children) = state.parent_to_children.remove(fn_id) {
            for child_id in children {
                if let Some(parents) = state.child_to_parents.get_mut(&child_id) {
                    parents.remove(fn_id);
                    if parents.is_empty() {
                        state.child_to_parents.remove(&child_id);
                    }
                }
            }
        }
        if let Some(parents) = state.child_to_parents.remove(fn_id) {
            for parent_id in parents {
                if let Some(children) = state.parent_to_children.get_mut(&parent_id) {
                    children.remove(fn_id);
                    if children.is_empty() {
                        state.parent_to_children.remove(&parent_id);
                    }
                }
            }
        }
    }

    fn clean_osr_metadata(&self, fn_id: &str) {
        self.state.lock().unwrap().loop_counters.remove(fn_id);
    }

    fn clean_metadata(&self, fn_id: &str) {
        let mut state = self.state.lock().unwrap();
        state.polymorphic_counters.remove(fn_id);
        state.global_invocation_counters.remove(fn_id);
        state.deopt_counters.remove(fn_id);
        state.error_counters.remove(fn_id);
        state.active_invocations.remove(fn_id);
        drop(state);
        self.clean_inlining_dependencies(fn_id);
    }

    fn register_dependency(&self, parent_id: &str, child_id: &str) {
        let mut state = self.state.lock().unwrap();
        state.parent_to_children.entry(parent_id.to_string()).or_default().insert(child_id.to_string());
        state.child_to_parents.entry(child_id.to_string()).or_default().insert(parent_id.to_string());
    }

    fn invalidate_parents(&self, child_id: &str, visited: &mut HashSet<String>) {
        if visited.contains(child_id) {
            return;
        }
        visited.insert(child_id.to_string());

        let mut state = self.state.lock().unwrap();
        if let Some(parents) = state.child_to_parents.remove(child_id) {
            for parent_id in parents {
                if let Some(children) = state.parent_to_children.get_mut(&parent_id) {
                    children.remove(child_id);
                }
                drop(state);
                self.deoptimize_total(&parent_id, visited);
                state = self.state.lock().unwrap();
            }
        }
    }

    fn handle_error(&self, fn_obj: &dyn HighLevelFunction, error: &str) {
        if error.contains("STACK OVERFLOW") {
            return;
        }
        let fn_id = fn_obj.id();
        let mut state = self.state.lock().unwrap();
        let err_count = *state.error_counters.entry(fn_id.clone()).and_modify(|e| *e += 1).or_insert(1);
        println!("[🛡️ JIT CAUGHT] Isu pada '{}' (Akumulasi Isu: {}/{})", fn_id, err_count, self.max_error_threshold);

        if err_count >= self.max_error_threshold {
            println!("[☣️ QUARANTINE] '{}' dimasukkan ke karantina.", fn_id);
            state.quarantined_functions.insert(fn_id.clone());
            state.compiled_cache.remove(&fn_id);
            state.osr_cache.remove(&fn_id);
            drop(state);

            let mut visited = HashSet::new();
            self.invalidate_parents(&fn_id, &mut visited);
            self.clean_metadata(&fn_id);
        }
    }

    fn create_budget(&self, parent_budget: Option<ExecutionBudget>, override_ms: Option<f64>) -> ExecutionBudget {
        if let Some(b) = parent_budget {
            b
        } else {
            let limit = override_ms.unwrap_or(self.default_time_budget_ms);
            ExecutionBudget {
                deadline: Instant::now() + Duration::from_millis(limit as u64),
                remaining_ms: limit,
            }
        }
    }

    fn check_budget(&self, budget: &ExecutionBudget) -> Result<(), String> {
        if Instant::now() > budget.deadline {
            return Err(format!("[⏱️ WATCHDOG] Execution budget exceeded ({}ms). Function terminated.", budget.remaining_ms));
        }
        Ok(())
    }

    fn track_active_invocation(&self, fn_id: &str) -> ActiveInvocationToken {
        let mut state = self.state.lock().unwrap();
        *state.active_invocations.entry(fn_id.to_string()).or_insert(0) += 1;
        ActiveInvocationToken {
            fn_id: fn_id.to_string(),
            engine_state: self.state.clone(),
        }
    }

    pub fn execute(&self, fn_obj: &dyn HighLevelFunction, args: Vec<Val>) -> Result<Val, String> {
        self.execute_internal(fn_obj, None, 0, self.create_budget(None, None), args)
    }

    pub fn execute_with_budget(&self, fn_obj: &dyn HighLevelFunction, budget_ms: f64, args: Vec<Val>) -> Result<Val, String> {
        self.execute_internal(fn_obj, None, 0, self.create_budget(None, Some(budget_ms)), args)
    }

    fn execute_internal(
        &self,
        fn_obj: &dyn HighLevelFunction,
        caller_id: Option<String>,
        depth: usize,
        budget: ExecutionBudget,
        args: Vec<Val>,
    ) -> Result<Val, String> {
        self.run_garbage_collector_check();
        self.check_budget(&budget)?;

        let fn_id = fn_obj.id();

        {
            let state = self.state.lock().unwrap();
            if state.blacklisted_polymorphic_functions.contains(&fn_id) {
                drop(state);
                return self.execute_in_sandbox(fn_obj, args, false, depth, budget);
            }

            if state.quarantined_functions.contains(&fn_id) {
                return Err(format!("[🚫 SANDBOX BLOCK] '{}' diblokir karena dimasukkan ke karantina.", fn_id));
            }
        }

        if let Some(ref cid) = caller_id {
            if cid != &fn_id && fn_obj.is_inlineable() {
                self.register_dependency(cid, &fn_id);
            }
        }

        let current_signature = get_strict_signature(&args);
        let mut serialization_state = false;
        let input_key = args.iter().map(|arg| arg.serialize_arg(0, &mut serialization_state)).collect::<Vec<_>>().join(",");
        let is_memoizable = !serialization_state;

        let _release_active = self.track_active_invocation(&fn_id);

        // ==================== TIER 2: OPTIMIZED JIT ====================
        let mut has_compiled = false;
        {
            let state = self.state.lock().unwrap();
            if state.compiled_cache.contains_key(&fn_id) {
                has_compiled = true;
            }
        }

        if has_compiled {
            let (tier, has_signature, has_baseline, variants_len) = {
                let mut state = self.state.lock().unwrap();
                let now = Instant::now();
                let (tier, has_signature, has_baseline, variants_len) = {
                    let compiled = state.compiled_cache.get_mut(&fn_id).unwrap();
                    compiled.last_used = now;
                    compiled.access_count += 1;
                    let tier = match compiled.tier {
                        Tier::OPTIMIZED => Tier::OPTIMIZED,
                        Tier::BASELINE => Tier::BASELINE,
                    };
                    let has_signature = compiled.variants.contains_key(&current_signature);
                    let has_baseline = compiled.baseline.is_some();
                    let variants_len = compiled.variants.len();
                    (tier, has_signature, has_baseline, variants_len)
                };
                state.clock_use_bits.insert(fn_id.clone(), true);
                (tier, has_signature, has_baseline, variants_len)
            };

            match tier {
                Tier::OPTIMIZED => {
                    if has_signature {
                        let cached_val = {
                            let state = self.state.lock().unwrap();
                            let compiled = state.compiled_cache.get(&fn_id).unwrap();
                            let variant = compiled.variants.get(&current_signature).unwrap();
                            if is_memoizable {
                                variant.memo_cache.get(&input_key).cloned()
                            } else {
                                None
                            }
                        };

                        if let Some(val) = cached_val {
                            return Ok(val);
                        }

                        // Execute sandbox
                        let result = self.execute_in_sandbox(fn_obj, args.clone(), false, depth, budget.clone());
                        match result {
                            Ok(res) => {
                                if is_memoizable {
                                    let mut state = self.state.lock().unwrap();
                                    if let Some(compiled_again) = state.compiled_cache.get_mut(&fn_id) {
                                        if let Some(v) = compiled_again.variants.get_mut(&current_signature) {
                                            if v.memo_cache.len() >= self.max_memo_cache_size {
                                                if let Some(oldest_key) = v.memo_cache.keys().next().cloned() {
                                                    v.memo_cache.remove(&oldest_key);
                                                }
                                            }
                                            v.memo_cache.insert(input_key, res.clone());
                                        }
                                    }
                                }
                                return Ok(res);
                            }
                            Err(err) => {
                                if err.contains("STACK OVERFLOW") {
                                    return Err(err);
                                }
                                self.deoptimize_variant(&fn_id, &current_signature);
                                return self.try_baseline_or_sandbox(fn_obj, args, true, depth, budget, &current_signature);
                            }
                        }
                    } else if variants_len >= self.max_polymorphic_slots {
                        self.deoptimize_to_baseline(&fn_id);
                        return self.execute_internal(fn_obj, caller_id, depth, budget, args);
                    }
                }
                Tier::BASELINE => {
                    if has_baseline {
                        {
                            let mut state = self.state.lock().unwrap();
                            if let Some(compiled) = state.compiled_cache.get_mut(&fn_id) {
                                if let Some(ref mut baseline) = compiled.baseline {
                                    baseline.last_used = Instant::now();
                                    baseline.access_count += 1;
                                }
                            }
                        }
                        self.profile_function(fn_obj, &current_signature);
                        return self.run_baseline(fn_obj, args, depth, budget);
                    }
                }
            }
        }

        // ==================== TIER 0: OSR ====================
        if fn_obj.is_loop_heavy() {
            let loop_count = {
                let mut state = self.state.lock().unwrap();
                *state.loop_counters.entry(fn_id.clone()).and_modify(|e| *e += 1).or_insert(1)
            };

            let mut has_osr = false;
            {
                let state = self.state.lock().unwrap();
                if state.osr_cache.contains_key(&fn_id) {
                    has_osr = true;
                }
            }

            if loop_count >= self.osr_threshold && !has_osr {
                println!("[🔄 OSR COMPILE] Mengoptimasi perulangan berat pada '{}'", fn_id);
                {
                    let mut state = self.state.lock().unwrap();
                    state.osr_cache.insert(fn_id.clone(), CompiledOSR {
                        id: fn_id.clone(),
                        last_used: Instant::now(),
                        access_count: 1,
                    });
                }
                self.add_to_clock_queue(fn_id.clone());
                has_osr = true;
            }

            if has_osr {
                self.profile_function(fn_obj, &current_signature);
                {
                    let mut state = self.state.lock().unwrap();
                    if let Some(osr) = state.osr_cache.get_mut(&fn_id) {
                        osr.last_used = Instant::now();
                        osr.access_count += 1;
                    }
                    state.clock_use_bits.insert(fn_id.clone(), true);
                }

                let result = self.execute_in_sandbox(fn_obj, args.clone(), false, depth, budget.clone());
                match result {
                    Ok(res) => return Ok(res),
                    Err(err) => {
                        if err.contains("STACK OVERFLOW") {
                            return Err(err);
                        }
                        self.state.lock().unwrap().osr_cache.remove(&fn_id);
                        self.handle_error(fn_obj, &err);
                        return self.execute_in_sandbox(fn_obj, args, true, depth, budget);
                    }
                }
            }
        }

        // ==================== TIER 0: INTERPRETED ====================
        self.profile_function(fn_obj, &current_signature);
        self.execute_in_sandbox(fn_obj, args, false, depth, budget)
    }

    fn try_baseline_or_sandbox(
        &self,
        fn_obj: &dyn HighLevelFunction,
        args: Vec<Val>,
        is_retry: bool,
        depth: usize,
        budget: ExecutionBudget,
        _signature: &str,
    ) -> Result<Val, String> {
        let fn_id = fn_obj.id();
        let has_baseline = {
            let state = self.state.lock().unwrap();
            state.compiled_cache.get(&fn_id)
                .and_then(|c| c.baseline.as_ref())
                .is_some()
        };

        if has_baseline {
            self.run_baseline(fn_obj, args, depth, budget)
        } else {
            self.execute_in_sandbox(fn_obj, args, is_retry, depth, budget)
        }
    }

    fn run_baseline(
        &self,
        fn_obj: &dyn HighLevelFunction,
        args: Vec<Val>,
        depth: usize,
        budget: ExecutionBudget,
    ) -> Result<Val, String> {
        let fn_id = fn_obj.id();
        let result = self.execute_in_sandbox(fn_obj, args.clone(), false, depth, budget.clone());
        match result {
            Ok(res) => Ok(res),
            Err(err) => {
                if err.contains("STACK OVERFLOW") {
                    return Err(err);
                }
                {
                    let mut state = self.state.lock().unwrap();
                    if let Some(compiled) = state.compiled_cache.get_mut(&fn_id) {
                        compiled.baseline = None;
                    }
                }
                self.execute_in_sandbox(fn_obj, args, true, depth, budget)
            }
        }
    }

    fn execute_in_sandbox(
        &self,
        fn_obj: &dyn HighLevelFunction,
        args: Vec<Val>,
        _is_retry: bool,
        depth: usize,
        budget: ExecutionBudget,
    ) -> Result<Val, String> {
        if depth >= self.max_stack_depth {
            return Err(format!("[💥 STACK OVERFLOW] JIT Stack Overflow Protected di kedalaman {}!", depth));
        }

        let ctx = SecureEngineContext {
            execute_fn: Arc::new({
                let engine_clone = self.clone();
                let caller_id = fn_obj.id();
                move |inner_fn, inner_args| {
                    engine_clone.execute_internal(inner_fn, Some(caller_id.clone()), depth + 1, budget.clone(), inner_args)
                }
            }),
        };

        match fn_obj.body(&ctx, args) {
            Ok(res) => Ok(res),
            Err(err) => {
                self.handle_error(fn_obj, &err);
                Err(err)
            }
        }
    }

    fn profile_function(&self, fn_obj: &dyn HighLevelFunction, current_signature: &str) {
        let fn_id = fn_obj.id();
        {
            let state = self.state.lock().unwrap();
            if state.blacklisted_polymorphic_functions.contains(&fn_id) {
                return;
            }
        }

        let (global_count, current_count) = {
            let mut state = self.state.lock().unwrap();
            let global_count = *state.global_invocation_counters.entry(fn_id.clone()).and_modify(|e| *e += 1).or_insert(1);
            let signature_map = state.polymorphic_counters.entry(fn_id.clone()).or_default();
            let current_count = *signature_map.entry(current_signature.to_string()).and_modify(|e| *e += 1).or_insert(1);
            (global_count, current_count)
        };

        if current_count >= self.jit_threshold {
            self.state.lock().unwrap().global_invocation_counters.insert(fn_id.clone(), 0);
            self.profile_and_compile_polymorphic(fn_obj, current_signature);
        } else if global_count >= self.max_global_slow_path_attempts {
            let mut state = self.state.lock().unwrap();
            state.blacklisted_polymorphic_functions.insert(fn_id.clone());
            state.compiled_cache.remove(&fn_id);
        }
    }

    fn profile_and_compile_polymorphic(&self, fn_obj: &dyn HighLevelFunction, signature: &str) {
        let fn_id = fn_obj.id();
        let mut state = self.state.lock().unwrap();
        let mut compiled = state.compiled_cache.remove(&fn_id).unwrap_or_else(|| CompiledCodePolymorphic {
            id: fn_id.clone(),
            tier: Tier::OPTIMIZED,
            variants: HashMap::new(),
            baseline: Some(BaselineVariant {
                last_used: Instant::now(),
                access_count: 1,
            }),
            last_used: Instant::now(),
            access_count: 1,
        });

        compiled.tier = Tier::OPTIMIZED;
        compiled.variants.insert(signature.to_string(), OptimizedVariant {
            expected_signature: signature.to_string(),
            memo_cache: BTreeMap::new(),
        });

        state.compiled_cache.insert(fn_id.clone(), compiled);
        drop(state);
        self.add_to_clock_queue(fn_id);
    }

    fn deoptimize_to_baseline(&self, fn_id: &str) {
        let mut state = self.state.lock().unwrap();
        if let Some(compiled) = state.compiled_cache.get_mut(fn_id) {
            if matches!(compiled.tier, Tier::BASELINE) {
                return;
            }
            println!("[📉 DEOPT] '{}' turun dari OPTIMIZED → BASELINE (terlalu polymorphic).", fn_id);
            compiled.tier = Tier::BASELINE;
            compiled.variants.clear();
            state.polymorphic_counters.remove(fn_id);

            let deopt_count = *state.deopt_counters.entry(fn_id.to_string()).and_modify(|e| *e += 1).or_insert(1);
            drop(state);
            let mut visited = HashSet::new();
            self.invalidate_parents(fn_id, &mut visited);

            if deopt_count >= self.max_deopt_threshold {
                let mut state = self.state.lock().unwrap();
                state.blacklisted_polymorphic_functions.insert(fn_id.to_string());
                state.compiled_cache.remove(fn_id);
            }
        }
    }

    fn deoptimize_variant(&self, fn_id: &str, signature: &str) {
        let mut trigger_deopt = false;
        {
            let mut state = self.state.lock().unwrap();
            if let Some(compiled) = state.compiled_cache.get_mut(fn_id) {
                compiled.variants.remove(signature);
                if compiled.variants.is_empty() {
                    trigger_deopt = true;
                }
            }
        }
        if trigger_deopt {
            self.deoptimize_to_baseline(fn_id);
        }
    }

    fn deoptimize_total(&self, fn_id: &str, visited: &mut HashSet<String>) {
        let mut state = self.state.lock().unwrap();
        if let Some(compiled) = state.compiled_cache.get_mut(fn_id) {
            if matches!(compiled.tier, Tier::BASELINE) {
                return;
            }
            compiled.tier = Tier::BASELINE;
            compiled.variants.clear();
            state.polymorphic_counters.remove(fn_id);

            let deopt_count = *state.deopt_counters.entry(fn_id.to_string()).and_modify(|e| *e += 1).or_insert(1);
            drop(state);
            self.invalidate_parents(fn_id, visited);

            if deopt_count >= self.max_deopt_threshold {
                let mut state = self.state.lock().unwrap();
                state.blacklisted_polymorphic_functions.insert(fn_id.to_string());
                state.compiled_cache.remove(fn_id);
            }
        }
    }

    fn run_garbage_collector_check(&self) {
        let (mut total_cache_items, mut current_memory_mb) = {
            let cache_len = {
                let state = self.state.lock().unwrap();
                state.compiled_cache.len() + state.osr_cache.len()
            };
            (cache_len, self.get_memory_usage_mb())
        };

        let mut effective_max_cache_size = self.max_cache_size;
        if total_cache_items > 0 {
            let average_memory_per_item = current_memory_mb / total_cache_items as f64;
            if average_memory_per_item > 0.0 {
                let state = self.state.lock().unwrap();
                let safe_cache_capacity = ((state.max_memory_mb * 0.9) / average_memory_per_item).floor() as usize;
                effective_max_cache_size = self.max_cache_size.min(safe_cache_capacity).max(1);
            }
        }

        let (max_memory_mb, target_memory_mb) = {
            let state = self.state.lock().unwrap();
            (state.max_memory_mb, state.max_memory_mb * 0.8)
        };
        let target_cache_size = ((effective_max_cache_size as f64 * 0.8).floor() as usize).max(1);

        if total_cache_items > effective_max_cache_size || current_memory_mb > max_memory_mb {
            println!(
                "[♻️ GC TRIGGERED] Memori: {:.2}MB/{:.2}MB, Cache: {}/{} (Limit Config: {}). membersihkan hingga Watermark.",
                current_memory_mb, max_memory_mb, total_cache_items, effective_max_cache_size, self.max_cache_size
            );

            while total_cache_items > target_cache_size || current_memory_mb > target_memory_mb {
                let has_evicted = self.evict_one_item();
                if !has_evicted {
                    break;
                }
                let state = self.state.lock().unwrap();
                total_cache_items = state.compiled_cache.len() + state.osr_cache.len();
                drop(state);
                current_memory_mb = self.get_memory_usage_mb();
            }
        }
    }

    fn evict_one_item(&self) -> bool {
        let mut state = self.state.lock().unwrap();
        let mut evictable_keys = Vec::new();
        for k in state.compiled_cache.keys() {
            if !state.active_invocations.contains_key(k) {
                evictable_keys.push(k.clone());
            }
        }
        for k in state.osr_cache.keys() {
            if !state.active_invocations.contains_key(k) {
                evictable_keys.push(k.clone());
            }
        }

        if evictable_keys.is_empty() {
            return false;
        }

        let has_clock_elems = {
            let cmp_compiled = &state.compiled_cache;
            let cmp_osr = &state.osr_cache;
            let mut keys_to_retain = Vec::new();
            for k in &state.clock_queue {
                if cmp_compiled.contains_key(k) || cmp_osr.contains_key(k) {
                    keys_to_retain.push(k.clone());
                }
            }
            state.clock_queue = keys_to_retain;
            !state.clock_queue.is_empty()
        };

        if !has_clock_elems {
            state.clock_queue = evictable_keys.clone();
            state.clock_hand = 0;
        }

        let mut attempts = 0;
        let max_attempts = state.clock_queue.len() * 2;

        while attempts < max_attempts {
            if state.clock_hand >= state.clock_queue.len() {
                state.clock_hand = 0;
            }

            let key = state.clock_queue[state.clock_hand].clone();
            if (!state.compiled_cache.contains_key(&key) && !state.osr_cache.contains_key(&key)) || state.active_invocations.contains_key(&key) {
                state.clock_hand += 1;
                attempts += 1;
                continue;
            }

            let use_bit = *state.clock_use_bits.get(&key).unwrap_or(&true);
            if use_bit {
                state.clock_use_bits.insert(key, false);
                state.clock_hand += 1;
                attempts += 1;
            } else {
                let target_cache = if state.compiled_cache.contains_key(&key) { "JIT" } else { "OSR" };
                drop(state);
                self.perform_eviction(&key, target_cache);
                state = self.state.lock().unwrap();
                state.clock_hand += 1;
                return true;
            }
        }

        // Fallback with Tie-Breaker
        evictable_keys.sort_by(|a, b| {
            let entry_a_opt = state.compiled_cache.get(a).map(|c| (c.access_count, c.last_used));
            let entry_b_opt = state.compiled_cache.get(b).map(|c| (c.access_count, c.last_used));
            let entry_a = entry_a_opt.unwrap_or_else(|| {
                let os = state.osr_cache.get(a).unwrap();
                (os.access_count, os.last_used)
            });
            let entry_b = entry_b_opt.unwrap_or_else(|| {
                let os = state.osr_cache.get(b).unwrap();
                (os.access_count, os.last_used)
            });

            if entry_a.0 != entry_b.0 {
                entry_a.0.cmp(&entry_b.0)
            } else {
                entry_a.1.cmp(&entry_b.1)
            }
        });

        let fallback_key = evictable_keys[0].clone();
        let target_cache = if state.compiled_cache.contains_key(&fallback_key) { "JIT" } else { "OSR" };
        drop(state);
        self.perform_eviction(&fallback_key, target_cache);
        true
    }

    fn perform_eviction(&self, key: &str, target_cache: &str) {
        if target_cache == "JIT" {
            self.state.lock().unwrap().compiled_cache.remove(key);
            let mut visited = HashSet::new();
            self.invalidate_parents(key, &mut visited);
            self.clean_metadata(key);
        } else {
            self.state.lock().unwrap().osr_cache.remove(key);
            self.clean_osr_metadata(key);
        }
        let mut state = self.state.lock().unwrap();
        state.clock_use_bits.remove(key);
        state.clock_queue.retain(|k| k != key);
        println!("[♻️ GC CLEANED - CLOCK] Menghapus cache & metadata '{}' ({}).", key, target_cache);
    }

    // Exposed helpers for testing
    pub fn get_active_invocations_map(&self) -> HashMap<String, usize> {
        self.state.lock().unwrap().active_invocations.clone()
    }

    pub fn get_parent_to_children_map(&self) -> HashMap<String, HashSet<String>> {
        self.state.lock().unwrap().parent_to_children.clone()
    }

    pub fn get_child_to_parents_map(&self) -> HashMap<String, HashSet<String>> {
        self.state.lock().unwrap().child_to_parents.clone()
    }

    pub fn force_register_dependency(&self, parent_id: &str, child_id: &str) {
        self.register_dependency(parent_id, child_id);
    }

    pub fn force_deoptimize_total(&self, fn_id: &str) {
        let mut visited = HashSet::new();
        self.deoptimize_total(fn_id, &mut visited);
    }

    pub fn force_invalidate_parents(&self, child_id: &str) {
        let mut visited = HashSet::new();
        self.invalidate_parents(child_id, &mut visited);
    }

    pub fn force_gc(&self) {
        while {
            let state = self.state.lock().unwrap();
            state.compiled_cache.len() + state.osr_cache.len() > 0
        } {
            if !self.evict_one_item() {
                break;
            }
        }
    }
}

struct ActiveInvocationToken {
    fn_id: String,
    engine_state: Arc<Mutex<EngineState>>,
}

impl Drop for ActiveInvocationToken {
    fn drop(&mut self) {
        let mut state = self.engine_state.lock().unwrap();
        if let Some(count) = state.active_invocations.get_mut(&self.fn_id) {
            if *count > 1 {
                *count -= 1;
            } else {
                state.active_invocations.remove(&self.fn_id);
            }
        }
    }
}
