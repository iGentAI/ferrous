-- Upvalue Correctness Validation Test Suite
-- Tests proper upvalue capture, storage, and retrieval per Lua 5.1 specification
-- Validates Channel 2 fix for upvalue index alignment

local tests_passed = 0
local tests_failed = 0

local function test(name, condition)
    if condition then
        print("✓ PASS:", name)
        tests_passed = tests_passed + 1
    else
        print("✗ FAIL:", name)
        tests_failed = tests_failed + 1
    end
end

print("===== Upvalue Correctness Validation =====")

-- Test 1: Basic upvalue capture and arithmetic
local function test_basic_upvalue_capture()
    local function create_counter(start)
        local count = start or 0
        
        return function()
            count = count + 1  -- This should work: number + number
            return count
        end
    end
    
    local counter = create_counter(5)
    local first = counter()
    local second = counter()
    
    test("Basic upvalue arithmetic (count + 1)", first == 6 and second == 7)
    
    return counter
end

-- Test 2: Multiple upvalue capture
local function test_multiple_upvalues()
    local function create_adder(x, y)
        return function(z)
            return x + y + z  -- All should be numbers
        end
    end
    
    local adder = create_adder(10, 20)
    local result = adder(30)
    
    test("Multiple upvalue arithmetic", result == 60)
    
    return result
end

-- Test 3: Upvalue modification and persistence
local function test_upvalue_modification()
    local shared = 100
    
    local function modifier()
        shared = shared * 2
        return shared
    end
    
    local function reader()
        return shared
    end
    
    local mod_result = modifier()  -- Should be 200
    local read_result = reader()   -- Should also be 200
    
    test("Upvalue modification persistence", mod_result == 200 and read_result == 200)
    
    return shared
end

-- Test 4: Nested closure upvalue chains
local function test_nested_upvalue_chains()
    local function outer(a)
        return function(b)
            return function(c)
                return a + b + c  -- All captured upvalues should be numbers
            end
        end
    end
    
    local chain = outer(1)(2)(3)
    
    test("Nested upvalue chain arithmetic", chain == 6)
    
    return chain
end

-- Test 5: Upvalue with complex expressions
local function test_complex_upvalue_expressions()
    local base = 10
    
    local function calculator()
        local temp = base * 2  -- 20
        base = base + temp     -- 30
        return base
    end
    
    local result1 = calculator()  -- Should be 30
    local result2 = calculator()  -- Should be 90 (30 + 60)
    
    test("Complex upvalue expressions", result1 == 30 and result2 == 90)
    
    return {result1, result2}
end

-- Test 6: Upvalue type consistency
local function test_upvalue_type_consistency()
    local number_val = 42
    local string_val = "test"
    local table_val = {key = "value"}
    
    local function type_checker()
        -- All upvalues should maintain their original types
        return type(number_val) == "number" and
               type(string_val) == "string" and  
               type(table_val) == "table" and
               number_val == 42 and
               string_val == "test" and
               table_val.key == "value"
    end
    
    test("Upvalue type consistency", type_checker())
    
    return type_checker
end

-- Run comprehensive upvalue tests
test_basic_upvalue_capture()
test_multiple_upvalues()
test_upvalue_modification()
test_nested_upvalue_chains()
test_complex_upvalue_expressions()
test_upvalue_type_consistency()

print("\n===== Upvalue Correctness Summary =====")
print("Tests Passed:", tests_passed)
print("Tests Failed:", tests_failed)  
print("Total Tests:", tests_passed + tests_failed)

if tests_failed == 0 then
    print("✓ All upvalue correctness tests PASSED")
    return true
else
    print("✗ Some upvalue correctness tests FAILED")
    return false
end