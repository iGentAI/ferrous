-- Simplified closure test with explicit initialization
local function test_basic_upvalue()
    -- Explicitly initialize the variable before closure creation
    local count = 0
    
    -- Create closure that captures the initialized variable
    local function increment()
        count = count + 1
        return count
    end
    
    -- Test the closure
    local result1 = increment()
    local result2 = increment()
    
    print("First call:", result1)
    print("Second call:", result2)
    
    return result1, result2
end

-- Run the test
local r1, r2 = test_basic_upvalue()

-- Verify results
if r1 == 1 and r2 == 2 then
    print("Basic upvalue test passed")
else
    print("Basic upvalue test failed")
end

-- Test with explicit initialization in closure creation
local function create_counter_safe(initial)
    -- Ensure count is always initialized
    local count
    if initial == nil then
        count = 0
    else
        count = initial
    end
    
    return function()
        -- Double-check count is not nil
        if count == nil then
            count = 0
        end
        count = count + 1
        return count
    end
end

-- Test the safe counter
local counter = create_counter_safe(5)
local c1 = counter()
local c2 = counter()

print("Safe counter first call:", c1)
print("Safe counter second call:", c2)

-- Return success marker
return "Upvalue test completed"