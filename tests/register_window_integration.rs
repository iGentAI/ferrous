//! Integration tests for the enhanced register window system
//! 
//! This test demonstrates all the key features working together:
//! - Window recycling for efficient memory usage
//! - Protection guards with RAII cleanup
//! - Debugging and visualization tools
//! - Error handling and recovery

use ferrous::lua::register_window::*;
use ferrous::lua::value::Value;
use ferrous::lua::error::{LuaResult, LuaError};
use ferrous::lua::vm::LuaVM;

/// Helper function to create a string value using VM
fn create_string(vm: &mut LuaVM, s: &str) -> LuaResult<Value> {
    let handle = vm.create_string(s)?;
    Ok(Value::String(handle))
}

/// Simulates a realistic VM execution scenario with nested function calls
#[test]
fn test_complete_register_window_system() -> LuaResult<()> {
    println!("\n╔════════════════════════════════════════════════╗");
    println!("║  REGISTER WINDOW SYSTEM INTEGRATION TEST       ║");
    println!("╚════════════════════════════════════════════════╝\n");

    // Create VM instance for string creation
    let mut vm = LuaVM::new()?;

    // Create the register window system with moderate initial capacity
    let mut system = RegisterWindowSystem::new(512);
    
    // Configure debug settings for comprehensive tracking
    #[cfg(debug_assertions)]
    {
        system.configure_debug(DebugConfig {
            enable_timeline: true,
            max_timeline_entries: 1000,
            verbose_registers: true,
            track_value_changes: true,
        });
    }
    
    // Set recycling pool limits for testing
    system.set_pool_limits(5, 20);
    
    println!("=== Phase 1: Initial Setup ===");
    
    // Create main execution window (simulating main chunk)
    let main_window = system.use_named_window("main", 50)?;
    println!("Created main window with 50 registers");
    
    // Initialize some global values in main
    system.set_register(main_window, 0, Value::Number(100.0))?; // Global counter
    system.set_register(main_window, 1, create_string(&mut vm, "global_state")?)?;
    system.set_register(main_window, 2, Value::Boolean(true))?;
    
    // Protect critical globals
    {
        let mut guard = system.protection_guard(main_window)?;
        guard.protect_range(0, 3)?;
        println!("Protected global registers 0-2 in main window");
        
        // Verify protection works
        guard.with_system(|sys| {
            assert!(sys.set_register(main_window, 0, Value::Nil).is_err());
            println!("✓ Protection verification passed");
            Ok(())
        })?;
    } // Protection automatically removed when guard drops
    
    println!("\n=== Phase 2: Function Call Simulation ===");
    
    // Simulate nested function calls
    simulate_function_calls(&mut system, &mut vm, main_window)?;
    
    println!("\n=== Phase 3: Window Recycling Test ===");
    
    test_window_recycling(&mut system)?;
    
    println!("\n=== Phase 4: Error Handling and Recovery ===");
    
    test_error_recovery(&mut system)?;
    
    println!("\n=== Phase 5: Complex Protection Patterns ===");
    
    test_complex_protections(&mut system, &mut vm)?;
    
    println!("\n=== Phase 6: System Analysis ===");
    
    // Generate comprehensive debug report
    println!("{}", system.debug_report());
    
    // Check for issues
    let issues = system.detect_issues();
    let issue_report = system.format_issues(&issues);
    println!("\n{}", issue_report);
    
    // Verify final state
    println!("\n=== Final Statistics ===");
    let stats = system.get_stats();
    println!("Windows allocated: {}", stats.windows_allocated());
    println!("Peak window count: {}", stats.peak_window_count());
    println!("Max nesting depth: {}", stats.max_nesting_depth());
    println!("Protection violations: {}", stats.protection_violations());
    println!("Windows recycled: {}", stats.windows_recycled());
    println!("Recycling hits: {}", stats.recycling_hits());
    println!("Recycling misses: {}", stats.recycling_misses());
    
    let (pool_count, pool_sizes, hit_rate) = system.get_pool_stats();
    println!("\nRecycling pool:");
    println!("- Windows in pool: {}", pool_count);
    println!("- Distinct sizes: {}", pool_sizes);
    println!("- Hit rate: {:.1}%", hit_rate * 100.0);
    
    // Verify expectations
    assert!(stats.windows_recycled() > 0, "Should have recycled windows");
    assert!(stats.recycling_hits() > 0, "Should have recycling hits");
    assert!(hit_rate > 0.3, "Hit rate should be reasonable");
    assert!(stats.protection_violations() > 0, "Should have caught protection violations");
    
    println!("\n✅ All integration tests passed!");
    
    Ok(())
}

/// Simulates nested function calls with different window sizes
fn simulate_function_calls(system: &mut RegisterWindowSystem, vm: &mut LuaVM, main_window: usize) -> LuaResult<()> {
    // Function 1: Small utility function
    println!("\nCalling utility function...");
    let func1_window = system.use_named_window("utility_func", 10)?;
    
    // Copy parameter from main
    system.copy_register(main_window, 0, func1_window, 0)?;
    
    // Do some computation
    system.set_register(func1_window, 1, Value::Number(42.0))?;
    system.set_register(func1_window, 2, Value::Number(84.0))?;
    
    // Function 2: Larger processing function (nested call)
    println!("Calling processing function (nested)...");
    let func2_window = system.allocate_window(30)?;
    system.set_window_name(func2_window, "process_func")?;
    
    // Protect intermediate results in func2
    {
        let mut guard = system.protect_registers_guarded(func2_window, &[0, 1, 2, 3, 4])?;
        
        // Initialize protected registers
        guard.with_system(|sys| {
            for i in 5..10 {
                sys.set_register(func2_window, i, Value::Number(i as f64 * 10.0))?;
            }
            Ok(())
        })?;
        
        println!("Protected 5 registers in process_func");
        
        // Function 3: Deeply nested helper
        guard.with_system(|sys| {
            println!("Calling helper function (deeply nested)...");
            let helper_window = sys.allocate_window(5)?;
            
            // Do work in helper
            let helper_result = create_string(vm, "helper_result")?;
            sys.set_register(helper_window, 0, helper_result)?;
            
            // Return from helper
            sys.deallocate_window()?;
            println!("Returned from helper function");
            Ok(())
        })?;
    } // Protected registers automatically unprotected
    
    // Return from func2
    system.deallocate_window()?;
    println!("Returned from processing function");
    
    // Return from func1 
    system.deallocate_window()?;
    println!("Returned from utility function");
    
    // Verify we're back at main
    assert_eq!(system.current_window(), Some(main_window));
    
    Ok(())
}

/// Tests the window recycling system with various allocation patterns
fn test_window_recycling(system: &mut RegisterWindowSystem) -> LuaResult<()> {
    let _initial_stats = system.get_stats().clone();
    
    // Pattern 1: Allocate and immediately deallocate same sizes
    println!("\nTesting immediate recycling pattern...");
    for i in 0..5 {
        let window = system.allocate_window(20)?;
        system.set_register(window, 0, Value::Number(i as f64))?;
        system.deallocate_window()?;
    }
    
    let (pool_count, _, _) = system.get_pool_stats();
    println!("Pool after pattern 1: {} windows", pool_count);
    
    // Pattern 2: Allocate multiple, then deallocate in LIFO order
    println!("\nTesting LIFO deallocation pattern...");
    let mut windows = Vec::new();
    for size in [10, 20, 30, 20, 10] {
        windows.push(system.allocate_window(size)?);
    }
    
    // Fill with data
    for (i, &window) in windows.iter().enumerate() {
        system.set_register(window, 0, Value::Number(i as f64 * 100.0))?;
    }
    
    // Deallocate in reverse order
    for _ in 0..windows.len() {
        system.deallocate_window()?;
    }
    
    let (pool_count, pool_sizes, _) = system.get_pool_stats();
    println!("Pool after pattern 2: {} windows, {} sizes", pool_count, pool_sizes);
    
    // Pattern 3: Reuse recycled windows
    println!("\nTesting recycled window reuse...");
    let reuse_stats = system.get_stats().clone();
    
    // These should mostly come from the pool
    for _ in 0..10 {
        let size = if rand::random::<bool>() { 20 } else { 10 };
        let window = system.allocate_window(size)?;
        
        // Verify window is clean
        for i in 0..size {
            let val = system.get_register(window, i)?;
            assert_eq!(*val, Value::Nil, "Recycled window should be cleared");
        }
        
        system.deallocate_window()?;
    }
    
    let final_stats = system.get_stats();
    let new_hits = final_stats.recycling_hits() - reuse_stats.recycling_hits();
    println!("Recycling hits during reuse: {}", new_hits);
    assert!(new_hits >= 8, "Should have high hit rate for common sizes");
    
    // Test pool cleanup
    println!("\nTesting pool cleanup...");
    
    // Fill pool beyond limits
    for size in [40, 50, 60, 70, 80] {
        for _ in 0..3 {
            let _window = system.allocate_window(size)?;
            system.deallocate_window()?;
        }
    }
    
    let (before_clean, _, _) = system.get_pool_stats();
    system.clean_pool(false);
    let (after_clean, _, _) = system.get_pool_stats();
    
    println!("Pool before cleanup: {} windows", before_clean);
    println!("Pool after cleanup: {} windows", after_clean); 
    assert!(after_clean < before_clean, "Pool should be reduced");
    
    Ok(())
}

/// Tests error recovery with protection guards
fn test_error_recovery(system: &mut RegisterWindowSystem) -> LuaResult<()> {
    // Create a window for error testing
    let error_window = system.allocate_window(15)?;
    
    // Initialize some values
    for i in 0..10 {
        system.set_register(error_window, i, Value::Number(i as f64))?;
    }
    
    // Test 1: Protection violation recovery
    println!("\nTesting protection violation recovery...");
    let violation_count_before = system.get_stats().protection_violations();
    
    {
        let mut guard = system.protection_guard(error_window)?;
        guard.protect_range(0, 5)?;
        
        // Attempt operations that will fail
        let result = guard.with_system(|sys| -> Result<(), LuaError> {
            // This will succeed
            sys.set_register(error_window, 6, Value::Number(99.0))?;
            
            // This will fail - register is protected
            sys.set_register(error_window, 2, Value::Number(99.0))?;
            
            // This won't execute
            sys.set_register(error_window, 7, Value::Number(100.0))?;
            Ok(())
        });
        
        assert!(result.is_err(), "Should have protection violation");
        println!("✓ Protection violation correctly detected");
    } // Guard drops, protection removed
    
    // Verify protection was cleaned up
    assert!(system.set_register(error_window, 2, Value::Number(200.0)).is_ok());
    let violation_count_after = system.get_stats().protection_violations();
    assert_eq!(violation_count_after - violation_count_before, 1);
    
    // Test 2: Panic simulation with multiple guards
    println!("\nTesting panic recovery with nested guards...");
    
    let panic_test = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        let mut guard1 = system.protection_guard(error_window).unwrap();
        guard1.protect_register(8).unwrap();
        
        // Nested protection
        let _ = guard1.with_system(|sys| -> Result<(), LuaError> {
            let mut guard2 = sys.protection_guard(error_window).unwrap();
            guard2.protect_register(9).unwrap();
            
            // Simulate panic
            panic!("Simulated panic during nested operation");
        });
    }));
    
    assert!(panic_test.is_err(), "Should have panicked");
    println!("✓ Panic recovery test completed");
    
    // Both registers should be unprotected after panic
    assert!(system.set_register(error_window, 8, Value::Number(888.0)).is_ok());
    assert!(system.set_register(error_window, 9, Value::Number(999.0)).is_ok());
    
    // Clean up
    system.deallocate_window()?;
    
    Ok(())
}

/// Tests complex protection patterns with multiple windows
fn test_complex_protections(system: &mut RegisterWindowSystem, vm: &mut LuaVM) -> LuaResult<()> {
    println!("\nSetting up complex protection scenario...");
    
    // Create a hierarchy of windows
    let parent = system.use_named_window("parent_scope", 25)?;
    let child1 = system.use_named_window("child1_scope", 20)?;
    let child2 = system.use_named_window("child2_scope", 20)?;
    
    // Set up initial state
    for i in 0..10 {
        system.set_register(parent, i, Value::Number(i as f64 * 1.0))?;
        system.set_register(child1, i, Value::Number(i as f64 * 10.0))?;
        system.set_register(child2, i, Value::Number(i as f64 * 100.0))?;
    }
    
    // Complex protection pattern with interleaved guards
    {
        println!("Creating interleaved protection guards...");
        
        // Guard 1: Protect parent window registers
        let mut parent_guard = system.protection_guard(parent)?;
        parent_guard.protect_range(0, 5)?;
        
        // Do some work with child1
        parent_guard.with_system(|sys| {
            // Guard 2: Protect child1 registers
            let mut child1_guard = sys.protection_guard(child1)?;
            child1_guard.protect_range(5, 10)?;
            
            // Do work with child2 while others are protected
            child1_guard.with_system(|sys2| {
                // Can modify child2
                let modified_str = create_string(vm, "modified")?;
                sys2.set_register(child2, 0, modified_str)?;
                
                // Can modify unprotected areas
                sys2.set_register(parent, 10, Value::Boolean(true))?;
                sys2.set_register(child1, 0, Value::Boolean(false))?;
                
                // Cannot modify protected areas
                assert!(sys2.set_register(parent, 0, Value::Nil).is_err());
                assert!(sys2.set_register(child1, 7, Value::Nil).is_err());
                
                Ok(())
            })
        })?;
        
        println!("✓ Interleaved protections working correctly");
    } // All protections released
    
    // Verify all protections are cleared
    assert!(system.set_register(parent, 0, Value::Number(1000.0)).is_ok());
    assert!(system.set_register(child1, 7, Value::Number(2000.0)).is_ok());
    
    // Visualize the final hierarchy
    println!("\nFinal window hierarchy:");
    println!("{}", system.visualize_hierarchy());
    
    // Show detailed view of one window
    println!("\nDetailed view of parent window:");
    println!("{}", system.visualize_window_registers(parent));
    
    // Clean up windows
    system.deallocate_window()?; // child2
    system.deallocate_window()?; // child1
    system.deallocate_window()?; // parent
    
    Ok(())
}

/// Simulates a real-world VM execution pattern
#[test] 
fn test_realistic_vm_simulation() -> LuaResult<()> {
    println!("\n╔════════════════════════════════════════════════╗");
    println!("║        REALISTIC VM SIMULATION TEST            ║");
    println!("╚════════════════════════════════════════════════╝\n");

    // Create VM instance for string creation
    let mut vm = LuaVM::new()?;

    let mut system = RegisterWindowSystem::new(256);
    
    // Configure for production-like settings
    system.set_pool_limits(10, 50);
    
    // Main program window
    let main = system.use_named_window("_main", 50)?;
    
    // Global variables
    system.set_register(main, 0, Value::Number(0.0))?;  // Loop counter
    system.set_register(main, 1, create_string(&mut vm, "status")?)?;
    system.set_register(main, 2, Value::Boolean(true))?; // Running flag
    
    println!("=== Simulating VM execution loop ===\n");
    
    // Simulate execution loop
    for iteration in 0..5 {
        println!("--- Iteration {} ---", iteration);
        
        // Update loop counter
        system.set_register(main, 0, Value::Number(iteration as f64))?;
        
        // Simulate function call
        simulate_lua_function_call(&mut system, &mut vm, "process_data", 15)?;
        
        // Simulate coroutine yield/resume
        simulate_coroutine_operations(&mut system, &mut vm)?;
        
        // Simulate table operations with temporary windows
        simulate_table_operations(&mut system, &mut vm)?;
        
        // Check pool efficiency periodically
        if iteration % 2 == 0 {
            let (pool_count, _, hit_rate) = system.get_pool_stats();
            println!("Pool status: {} windows, {:.1}% hit rate", pool_count, hit_rate * 100.0);
        }
    }
    
    println!("\n=== VM Execution Summary ===");
    println!("{}", system.generate_usage_summary());
    
    // Verify healthy operation
    let stats = system.get_stats();
    let (_, _, hit_rate) = system.get_pool_stats();
    
    assert!(hit_rate > 0.5, "Pool hit rate should be good");
    assert!(stats.protection_violations() == 0, "No protection violations in normal operation");
    assert!(system.window_stack.len() == 1, "Should return to just main window");
    
    Ok(())
}

/// Simulates a Lua function call with local variables
fn simulate_lua_function_call(system: &mut RegisterWindowSystem, vm: &mut LuaVM, name: &str, local_count: usize) -> LuaResult<()> {
    println!("  Calling function: {}", name);
    
    let func_window = system.allocate_window(local_count + 10)?; // Locals + temps
    
    // Protect function arguments (first 3 locals)
    let mut arg_guard = system.protect_range_guarded(func_window, 0, 3.min(local_count))?;
    
    // Initialize arguments
    arg_guard.with_system(|sys| {
        for i in 3..local_count {
            sys.set_register(func_window, i, Value::Number(i as f64))?;
        }
        Ok(())
    })?;
    
    // Simulate function body execution
    arg_guard.with_system(|sys| {
        // Temporary calculations
        sys.set_register(func_window, local_count, Value::Number(42.0))?;
        sys.set_register(func_window, local_count + 1, Value::Number(84.0))?;
        
        // Nested function call
        let nested = sys.allocate_window(5)?;
        let nested_result = create_string(vm, "nested_result")?;
        sys.set_register(nested, 0, nested_result)?;
        sys.deallocate_window()?;
        
        Ok(())
    })?;
    
    // Function cleanup (guard drops here)
    drop(arg_guard);
    system.deallocate_window()?;
    
    println!("  Returned from: {}", name);
    Ok(())
}

/// Simulates coroutine operations
fn simulate_coroutine_operations(system: &mut RegisterWindowSystem, vm: &mut LuaVM) -> LuaResult<()> {
    println!("  Simulating coroutine operations...");
    
    // Coroutine state window
    let coro_window = system.use_named_window("coroutine_1", 30)?;
    
    // Simulate yield - protect coroutine state
    {
        let mut state_guard = system.protection_guard(coro_window)?;
        state_guard.protect_range(0, 10)?; // Protect yield values
        
        state_guard.with_system(|sys| {
            // Set yield values
            let yielded_str = create_string(vm, "yielded")?;
            sys.set_register(coro_window, 0, yielded_str)?;
            sys.set_register(coro_window, 1, Value::Number(123.0))?;
            
            // Temporary window for other operations while yielded
            let temp = sys.allocate_window(5)?;
            sys.set_register(temp, 0, Value::Boolean(true))?;
            sys.deallocate_window()?;
            
            Ok(())
        })?;
    } // State unprotected after yield
    
    // Simulate resume
    let resumed_str = create_string(vm, "resumed")?;
    system.set_register(coro_window, 15, resumed_str)?;
    
    // Clean up coroutine
    system.deallocate_window()?;
    
    Ok(())
}

/// Simulates table operations that use temporary windows
fn simulate_table_operations(system: &mut RegisterWindowSystem, vm: &mut LuaVM) -> LuaResult<()> {
    println!("  Simulating table operations...");
    
    // Small windows for table operations
    let sizes = [4, 6, 4, 8, 4, 6]; // Common sizes to test pooling
    
    for (i, &size) in sizes.iter().enumerate() {
        let temp = system.allocate_window(size)?;
        
        // Simulate table key/value pairs
        if size >= 2 {
            let key_str = create_string(vm, &format!("key_{}", i))?;
            system.set_register(temp, 0, key_str)?;
            system.set_register(temp, 1, Value::Number(i as f64))?;
        }
        
        system.deallocate_window()?;
    }
    
    Ok(())
}

/// Helper to create a random-like boolean (simple for testing)
mod rand {
    static mut COUNTER: u32 = 0;
    
    pub fn random<T>() -> bool {
        unsafe {
            COUNTER = COUNTER.wrapping_add(1);
            (COUNTER % 2) == 0
        }
    }
}