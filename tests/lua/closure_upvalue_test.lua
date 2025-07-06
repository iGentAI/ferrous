-- Simple closure test to verify upvalue implementation
local function create_counter(initial)
    local count = initial or 0
    return function()
        count = count + 1
        return count
    end
end

-- Create two counters to verify separate upvalue capture
local counter1 = create_counter(10)
local counter2 = create_counter(20)

-- Test counter1 - should increment from 10
local result1 = counter1()
local result2 = counter1()

-- Test counter2 - should increment from 20
local result3 = counter2()
local result4 = counter2()

-- Verify results
print("Counter1 first call:", result1)
print("Counter1 second call:", result2)
print("Counter2 first call:", result3)
print("Counter2 second call:", result4)

-- Test that upvalues are correctly shared between closures
local function create_shared_upvalue()
    local shared = 0
    
    -- Create two functions that share the same upvalue
    local function increment()
        shared = shared + 1
        return shared
    end
    
    local function get_value()
        return shared
    end
    
    return increment, get_value
end

local inc, get = create_shared_upvalue()

-- Test shared upvalues
print("Initial value:", get())
print("After increment:", inc())
print("Current value:", get())
print("After another increment:", inc())
print("Final value:", get())

-- Return values to verify
return {
    counter1 = { result1, result2 },
    counter2 = { result3, result4 },
    shared_upvalue = { 
        initial = get(),
        incremented = inc(), 
        final = get() 
    }
}