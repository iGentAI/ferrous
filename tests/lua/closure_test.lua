-- Closure and Upvalue Test Script
-- This test focuses on testing all aspects of closures and upvalues

-- =======================================
-- Basic Closures
-- =======================================

-- Test 1: Simple closure with one upvalue
function create_counter()
    local count = 0
    return function()
        count = count + 1
        return count
    end
end

local counter1 = create_counter()
local counter2 = create_counter()

-- Each counter should have its own independent upvalue
assert(counter1() == 1)
assert(counter1() == 2)
assert(counter2() == 1)  -- Counter2 has its own separate count

-- Test 2: Closure with multiple upvalues
function create_adder(x)
    return function(y)
        return x + y
    end
end

local add5 = create_adder(5)
local add10 = create_adder(10)

assert(add5(3) == 8)
assert(add10(3) == 13)

-- Test 3: Modifying upvalues
function create_accumulator(initial)
    local sum = initial
    local count = 0
    
    return function(value)
        sum = sum + value
        count = count + 1
        return sum, count
    end
end

local acc = create_accumulator(10)
local sum1, count1 = acc(5)  -- sum=15, count=1
local sum2, count2 = acc(7)  -- sum=22, count=2

assert(sum1 == 15 and count1 == 1)
assert(sum2 == 22 and count2 == 2)

-- =======================================
-- Nested Closures
-- =======================================

-- Test 4: Nested closures sharing upvalues
function outer_function(x)
    local y = x * 2
    
    local inner1 = function()
        return x + y  -- Both x and y are upvalues
    end
    
    local inner2 = function(z)
        x = x + z     -- Modifies the shared upvalue x
        return x
    end
    
    return inner1, inner2
end

local get_sum, modify_x = outer_function(5)
assert(get_sum() == 15)  -- 5 + (5*2)
assert(modify_x(2) == 7) -- x is now 7
assert(get_sum() == 17)  -- 7 + (5*2)

-- Test 5: Multi-level nested closures
function level1(a)
    return function(b)
        return function(c)
            return a + b + c
        end
    end
end

local lvl1 = level1(100)
local lvl2 = lvl1(20)
local lvl3 = lvl2(3)

assert(lvl3 == 123)

-- =======================================
-- Upvalue Closing
-- =======================================

-- Test 6: CLOSE instruction testing
function create_functions()
    local result = {}
    local data = {}
    
    -- These locals should become upvalues
    for i=1, 5 do
        local value = i * 10
        data[i] = value
        
        -- This creates a closure that captures 'value'
        result[i] = function() return value end
    end
    
    -- At the end of this function, all upvalues should be closed
    return result, data
end

local funcs, data = create_functions()

-- Each function should have captured the correct value
for i=1, 5 do
    assert(funcs[i]() == data[i])
end

-- Test 7: Shared upvalues across multiple closures
function make_counter_functions()
    local count = 0
    
    local function increment()
        count = count + 1
    end
    
    local function get_count()
        return count
    end
    
    return increment, get_count
end

local inc, get = make_counter_functions()
inc()
inc()
inc()
assert(get() == 3)

-- =======================================
-- Complex Scenarios
-- =======================================

-- Test 8: Function factory with environment setup
function create_environment(base)
    local env = {base = base}
    
    -- Create environment functions
    env.add = function(x) 
        env.base = env.base + x
        return env.base
    end
    
    env.subtract = function(x)
        env.base = env.base - x
        return env.base
    end
    
    env.get = function()
        return env.base
    end
    
    return env
end

local env1 = create_environment(100)
local env2 = create_environment(200)

assert(env1.add(50) == 150)
assert(env2.subtract(25) == 175)
assert(env1.get() == 150) -- Verify env1 wasn't affected by env2

-- Test 9: Mutually recursive closures
local is_even, is_odd

is_even = function(n)
    if n == 0 then return true end
    return is_odd(n - 1)
end

is_odd = function(n)
    if n == 0 then return false end
    return is_even(n - 1)
end

assert(is_even(4) == true)
assert(is_odd(5) == true)
assert(is_even(5) == false)

-- Test 10: Closures as object methods
function create_counter_object(start)
    local self = {count = start or 0}
    
    self.increment = function()
        self.count = self.count + 1
        return self.count
    end
    
    self.decrement = function()
        self.count = self.count - 1
        return self.count
    end
    
    self.get = function()
        return self.count
    end
    
    return self
end

local counter_obj = create_counter_object(10)
assert(counter_obj.increment() == 11)
assert(counter_obj.increment() == 12)
assert(counter_obj.decrement() == 11)
assert(counter_obj.get() == 11)

-- =======================================
-- Final Verification Report
-- =======================================

print("All closure and upvalue tests passed!")

return {
    basic_closures = counter2() == 2,
    multi_upvalues = add10(5) == 15,
    modify_upvalues = acc(3)[1] == 25, -- sum should be 25 after adding 3
    nested_closures = get_sum() == 17,
    multi_level = lvl3 == 123,
    closed_upvalues = funcs[3]() == 30,
    shared_upvalues = get() == 3,
    complex_environments = env2.get() == 175,
    mutual_recursion = is_odd(7) == true,
    object_methods = counter_obj.get() == 11
}