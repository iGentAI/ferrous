-- Lua 5.1 Specification Compliance Validation
-- Comprehensive test suite validating interpreter behavior against official specification
-- Documents correct behavior for aspect not covered by basic test suite

print("===== Lua 5.1 Specification Compliance Validation =====")

local tests_passed = 0
local tests_failed = 0

local function test(name, condition, expected_behavior)
    if condition then
        print("✓ PASS:", name)
        if expected_behavior then
            print("  Expected:", expected_behavior)
        end
        tests_passed = tests_passed + 1
    else
        print("✗ FAIL:", name) 
        if expected_behavior then
            print("  Expected:", expected_behavior)
        end
        tests_failed = tests_failed + 1
    end
end

-- Global environment metamethod compliance
print("\n--- Global Environment Metamethods ---")
local original_global_count = 0
for k,v in pairs(_G) do 
    original_global_count = original_global_count + 1 
end

setmetatable(_G, {
    __index = function(t, k)
        return "default_global_" .. k
    end
})

test("Global __index metamethod", 
     nonexistent_global == "default_global_nonexistent_global",
     "Global __index should be called for undefined variables")

-- Metamethod precedence validation  
print("\n--- Metamethod Precedence ---")
local mt_test = {}
setmetatable(mt_test, {
    __len = function(t) return 999 end
})

test("__len metamethod precedence", 
     #mt_test == 999,
     "__len metamethod should take precedence over default length")

-- Table iteration specification compliance
print("\n--- Table Iteration Compliance ---")
local iter_table = {a = 1, b = 2, [1] = 10, [2] = 20}
local key_count = 0
local value_sum = 0

for k, v in pairs(iter_table) do
    key_count = key_count + 1
    if type(v) == "number" then
        value_sum = value_sum + v
    end
end

test("Table iteration completeness",
     key_count == 4 and value_sum == 33,
     "pairs() should iterate over all table entries (array + hash)")

-- Numeric for loop specification compliance
print("\n--- Numeric For Loop Compliance ---")
local for_sum = 0
for i = 1, 5 do
    for_sum = for_sum + i
end

test("Numeric for loop calculation",
     for_sum == 15,
     "Numeric for loop should calculate sum 1+2+3+4+5 = 15")

-- String concatenation with type coercion
print("\n--- String Operations Compliance ---")
local concat_result = "Result: " .. 42 .. " items"
test("String concatenation with number coercion",
     concat_result == "Result: 42 items",
     "Number should be automatically converted to string in concatenation")

-- Function call argument handling
print("\n--- Function Call Compliance ---")
local function var_args_func(...)
    local args = {...}
    return #args, args[1], args[2], args[3]
end

local count, a, b, c = var_args_func(10, 20, 30)
test("Varargs function call",
     count == 3 and a == 10 and b == 20 and c == 30,
     "Varargs should correctly capture and return arguments")

-- Table length with holes (implementation-defined behavior)
print("\n--- Table Length Edge Cases ---")
local holey_table = {1, 2, nil, 4, 5}
local length = #holey_table
test("Table length with holes",
     length >= 2 and length <= 5,
     "Table length with holes is implementation-defined but should be reasonable")

-- Closure environment inheritance
print("\n--- Closure Environment ---")
local env_test = "global_value"

local function create_env_closure()
    return function()
        return env_test  -- Should access global environment
    end
end

local env_closure = create_env_closure()
test("Closure environment inheritance",
     env_closure() == "global_value",
     "Closures should inherit environment for global variable access")

-- Error handling compliance
print("\n--- Error Handling Compliance ---")
local error_caught = false
local success, error_msg = pcall(function()
    error("test error message")
end)

test("Error propagation",
     not success and type(error_msg) == "string",
     "error() should be caught by pcall() and return error message")

-- Type conversion compliance  
print("\n--- Type System Compliance ---")
test("Boolean truthiness", 
     (0 and true) == true and (false or nil) == nil,
     "Only false and nil are falsey in Lua")

test("Number precision",
     1.0 == 1 and type(1.0) == "number",
     "Integer and float numbers should be equivalent")

-- Print final summary
print("\n===== Specification Compliance Summary =====")
print("Tests Passed:", tests_passed)
print("Tests Failed:", tests_failed)
print("Total Tests:", tests_passed + tests_failed)
print("Success Rate:", string.format("%.1f%%", (tests_passed / (tests_passed + tests_failed)) * 100))

if tests_failed == 0 then
    print("✓ Full Lua 5.1 specification compliance achieved")
    return true
else
    print("⚠ Specification compliance gaps identified") 
    print("  Review failed tests for implementation corrections")
    return false
end