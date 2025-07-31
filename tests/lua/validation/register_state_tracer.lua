-- Register State Tracer Test  
-- Systematically traces register and upvalue state during closure operations
-- Designed to identify the exact contamination injection point

print("===== Register State Tracer Test =====")

-- Helper function to trace current state
local function trace_state(stage, detail)
    print(string.format("[TRACE %s] %s", stage, detail))
end

-- Test 1: Minimal contamination reproduction
print("\n--- Test 1: Minimal Contamination Reproduction ---")
function minimal_contamination_test()
    trace_state("T1.1", "Starting minimal test")
    
    -- This pattern is confirmed to work in simple cases
    local function simple_counter()
        local count = 0
        trace_state("T1.2", string.format("Local count initialized: %s (type: %s)", count, type(count)))
        
        local closure = function()
            trace_state("T1.3", string.format("GETUPVAL before arithmetic: %s (type: %s)", count, type(count)))
            count = count + 1
            trace_state("T1.4", string.format("GETUPVAL after arithmetic: %s (type: %s)", count, type(count)))
            return count
        end
        
        trace_state("T1.5", string.format("Closure created, type: %s", type(closure)))
        return closure
    end
    
    local counter = simple_counter()
    trace_state("T1.6", "About to execute closure")
    local result = counter()
    trace_state("T1.7", string.format("Closure executed, result: %s (type: %s)", result, type(result)))
    
    return result == 1
end

local test1_success = minimal_contamination_test()
trace_state("T1.RESULT", string.format("Minimal test success: %s", test1_success))

-- Test 2: Register reuse contamination probe  
print("\n--- Test 2: Register Reuse Contamination Probe ---")
function register_reuse_test()
    trace_state("T2.1", "Starting register reuse test")
    
    -- Pattern that triggers contamination in complex scenarios
    local function contaminating_pattern()
        trace_state("T2.2", "Entering contaminating pattern")
        
        -- First closure creation
        local function first_creator()
            local value1 = 100
            trace_state("T2.3", string.format("First value: %s (type: %s)", value1, type(value1)))
            
            return function()
                trace_state("T2.4", string.format("First GETUPVAL: %s (type: %s)", value1, type(value1)))
                local result = value1 + 1
                trace_state("T2.5", string.format("First result: %s (type: %s)", result, type(result)))
                return result
            end
        end
        
        local first_closure = first_creator()
        trace_state("T2.6", string.format("First closure created: %s", type(first_closure)))
        
        -- Critical: Execute first closure before creating second
        local first_result = first_closure()
        trace_state("T2.7", string.format("First closure result: %s (type: %s)", first_result, type(first_result)))
        
        -- Second closure creation (potential contamination point)
        local function second_creator()
            local value2 = 200
            trace_state("T2.8", string.format("Second value: %s (type: %s)", value2, type(value2)))
            
            return function()
                trace_state("T2.9", string.format("Second GETUPVAL PRE-CHECK: %s (type: %s)", value2, type(value2)))
                
                -- CONTAMINATION CHECK POINT
                if type(value2) ~= "number" then
                    trace_state("T2.ERROR", string.format("CONTAMINATION DETECTED: Expected number, got %s with value: %s", type(value2), tostring(value2)))
                    error("Register contamination detected in second closure")
                end
                
                local result = value2 + 1 
                trace_state("T2.10", string.format("Second result: %s (type: %s)", result, type(result)))
                return result
            end
        end
        
        local second_closure = second_creator()
        trace_state("T2.11", string.format("Second closure created: %s", type(second_closure)))
        
        -- Execute second closure (contamination trigger point)
        local second_result = second_closure()
        trace_state("T2.12", string.format("Second closure result: %s (type: %s)", second_result, type(second_result)))
        
        return first_result, second_result
    end
    
    local success, first_result, second_result = pcall(contaminating_pattern)
    
    if success then
        trace_state("T2.SUCCESS", string.format("Pattern completed: first=%s, second=%s", first_result, second_result))
        return first_result == 101 and second_result == 201
    else
        trace_state("T2.FAILURE", string.format("Pattern failed with error: %s", first_result))
        return false
    end
end

local test2_success = register_reuse_test() 
trace_state("T2.RESULT", string.format("Register reuse test success: %s", test2_success))

-- Test 3: Global vs Local closure contamination comparison
print("\n--- Test 3: Global vs Local Closure Comparison ---")
function global_vs_local_test()
    trace_state("T3.1", "Starting global vs local comparison")
    
    -- Global function scenario (known to trigger contamination)
    trace_state("T3.2", "Testing global function scenario")  
    
    -- This should be run to identify the difference
    local global_success = true
    local local_success = true
    
    -- Global function test (simulate the failing pattern)
    function global_create_counter()
        local count = 0
        trace_state("T3.3", string.format("Global - count initialized: %s (type: %s)", count, type(count)))
        
        return function()
            trace_state("T3.4", string.format("Global - GETUPVAL: %s (type: %s)", count, type(count)))
            if type(count) ~= "number" then
                trace_state("T3.ERROR.GLOBAL", "Global function contamination detected")
                global_success = false
                return nil
            end
            count = count + 1
            return count
        end
    end
    
    -- Local function test (control)
    local function local_create_counter()
        local count = 0  
        trace_state("T3.5", string.format("Local - count initialized: %s (type: %s)", count, type(count)))
        
        return function()
            trace_state("T3.6", string.format("Local - GETUPVAL: %s (type: %s)", count, type(count)))
            if type(count) ~= "number" then
                trace_state("T3.ERROR.LOCAL", "Local function contamination detected")  
                local_success = false
                return nil
            end
            count = count + 1
            return count
        end
    end
    
    -- Test both patterns
    local global_counter = global_create_counter()
    local global_result = global_counter()
    trace_state("T3.7", string.format("Global result: %s (success: %s)", global_result, global_success))
    
    local local_counter = local_create_counter()
    local local_result = local_counter()
    trace_state("T3.8", string.format("Local result: %s (success: %s)", local_result, local_success))
    
    return global_success and local_success
end

local test3_success = global_vs_local_test()
trace_state("T3.RESULT", string.format("Global vs local test success: %s", test3_success))

-- Final diagnostic assessment
print("\n===== Contamination Diagnostic Results =====")
print("Test 1 (Minimal):", test1_success) 
print("Test 2 (Register Reuse):", test2_success)
print("Test 3 (Global vs Local):", test3_success)

local all_success = test1_success and test2_success and test3_success

if all_success then
    print("✓ All tests PASSED - Contamination may be resolved or in other scenarios")
else
    print("✗ Contamination detected in specific scenarios:")
    if not test1_success then print("  - Minimal pattern contaminated") end
    if not test2_success then print("  - Register reuse pattern contaminated") end  
    if not test3_success then print("  - Global function pattern contaminated") end
end

return all_success