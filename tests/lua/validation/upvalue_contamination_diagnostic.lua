-- Upvalue Contamination Diagnostic Test
-- Systematic test to isolate exactly where corrupted values are coming from
-- Tests each stage of upvalue lifecycle with detailed tracing

print("===== Upvalue Contamination Diagnostic Test =====")

-- Stage 1: Single closure creation and execution (Control Test)
print("\n--- Stage 1: Single Closure Control Test ---")
function single_closure_test()
    local count = 42
    print("Stage 1 - Initial value:", count, type(count))
    
    local closure = function()
        print("Stage 1 - GETUPVAL retrieval:", count, type(count))
        local result = count + 1
        print("Stage 1 - Arithmetic result:", result, type(result))
        return result
    end
    
    print("Stage 1 - Closure created, type:", type(closure))
    local result = closure()
    print("Stage 1 - Final result:", result, type(result))
    return result
end

local stage1_result = single_closure_test()
print("Stage 1 SUCCESS:", stage1_result == 43)

-- Stage 2: Sequential closure creation (Isolation Test)  
print("\n--- Stage 2: Sequential Closure Creation Test ---")
function sequential_closure_test()
    -- First closure creation and execution
    local function create_first()
        local count1 = 100
        print("Stage 2a - First count:", count1, type(count1))
        return function() 
            print("Stage 2a - First GETUPVAL:", count1, type(count1))
            return count1 + 1 
        end
    end
    
    local first_closure = create_first()
    print("Stage 2a - First closure type:", type(first_closure))
    local first_result = first_closure()
    print("Stage 2a - First result:", first_result, type(first_result))
    
    -- Second closure creation and execution (potential contamination point)
    local function create_second()
        local count2 = 200
        print("Stage 2b - Second count:", count2, type(count2))
        return function() 
            print("Stage 2b - Second GETUPVAL:", count2, type(count2))
            return count2 + 1 
        end
    end
    
    local second_closure = create_second()
    print("Stage 2b - Second closure type:", type(second_closure))
    local second_result = second_closure()
    print("Stage 2b - Second result:", second_result, type(second_result))
    
    return first_result, second_result
end

local stage2_first, stage2_second = sequential_closure_test()
print("Stage 2 SUCCESS:", stage2_first == 101 and stage2_second == 201)

-- Stage 3: Immediate multiple closure creation (Contamination Trigger Test)
print("\n--- Stage 3: Multiple Closure Creation Test ---") 
function multiple_closure_test()
    local function create_counter(initial)
        print("Stage 3 - Creating counter with initial:", initial, type(initial))
        
        return function()
            print("Stage 3 - GETUPVAL retrieval:", initial, type(initial))
            initial = initial + 1
            print("Stage 3 - After increment:", initial, type(initial))
            return initial
        end
    end
    
    -- Create multiple closures in quick succession (likely contamination trigger)
    print("Stage 3 - Creating first counter...")
    local counter1 = create_counter(10)
    print("Stage 3 - First counter type:", type(counter1))
    
    print("Stage 3 - Creating second counter...")  
    local counter2 = create_counter(20)
    print("Stage 3 - Second counter type:", type(counter2))
    
    -- Execute first counter (potential contamination point)
    print("Stage 3 - Executing first counter...")
    local result1 = counter1()
    print("Stage 3 - First counter result:", result1, type(result1))
    
    -- Execute second counter (contamination check)
    print("Stage 3 - Executing second counter...")
    local result2 = counter2()
    print("Stage 3 - Second counter result:", result2, type(result2))
    
    return result1, result2
end

local stage3_first, stage3_second = multiple_closure_test()
local stage3_success = type(stage3_first) == "number" and type(stage3_second) == "number"
print("Stage 3 SUCCESS:", stage3_success)
print("Stage 3 DETAILS:", "first =", stage3_first, type(stage3_first), "second =", stage3_second, type(stage3_second))

-- Stage 4: Value contamination precise isolation
print("\n--- Stage 4: Contamination Point Isolation ---")
function contamination_isolation_test()
    local trace_count = 0
    
    local function make_traced_counter()
        trace_count = trace_count + 1
        local id = trace_count
        local value = id * 100  -- 100, 200, 300, etc.
        
        print("Stage 4 - Trace", id, "- Initial value:", value, type(value))
        
        local closure = function()
            print("Stage 4 - Trace", id, "- Pre-arithmetic value:", value, type(value))
            
            -- This is where contamination likely occurs
            if type(value) ~= "number" then
                print("*** CONTAMINATION DETECTED - Trace", id, "***")
                print("*** Expected number, got:", type(value), "***")
                print("*** Value content:", value, "***")
                error("CONTAMINATION: Expected number, got " .. type(value))
            end
            
            local result = value + 1
            print("Stage 4 - Trace", id, "- Post-arithmetic result:", result, type(result))
            return result
        end
        
        print("Stage 4 - Trace", id, "- Closure created successfully")
        return closure
    end
    
    -- Create and test multiple closures to identify exact contamination point
    local closures = {}
    
    print("Stage 4 - Creating closures...")
    for i = 1, 3 do
        closures[i] = make_traced_counter()
    end
    
    print("Stage 4 - Executing closures...")
    for i = 1, 3 do
        local success, result = pcall(closures[i])
        if success then
            print("Stage 4 - Closure", i, "SUCCESS:", result)
        else
            print("Stage 4 - Closure", i, "FAILED:", result)
            return false
        end
    end
    
    return true
end

local stage4_success = contamination_isolation_test()
print("Stage 4 SUCCESS:", stage4_success)

-- Final Assessment
print("\n===== Diagnostic Results =====") 
print("Stage 1 (Single):", stage1_result == 43)
print("Stage 2 (Sequential):", stage2_first == 101 and stage2_second == 201) 
print("Stage 3 (Multiple):", stage3_success)
print("Stage 4 (Isolation):", stage4_success)

local overall_success = (stage1_result == 43) and 
                       (stage2_first == 101 and stage2_second == 201) and 
                       stage3_success and 
                       stage4_success

if overall_success then
    print("✓ All diagnostic stages PASSED - No contamination detected")
else
    print("✗ Contamination detected in stages above")
    print("FAILURE ANALYSIS:")
    if stage1_result ~= 43 then print("  - Single closure stage failed") end
    if not (stage2_first == 101 and stage2_second == 201) then print("  - Sequential closure stage failed") end
    if not stage3_success then print("  - Multiple closure stage failed") end 
    if not stage4_success then print("  - Contamination isolation stage failed") end
end

return overall_success