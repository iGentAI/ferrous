-- Register Integrity Validation Test Suite
-- Tests correct register management and value persistence through complex operations
-- Validates Channel 1 fix for register window contamination after CALL operations

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

print("===== Register Integrity Validation =====")

-- Test 1: Table variable persistence through complex boolean expressions
local function test_table_persistence()
    local array_table = {1, 2, 3, 4, 5}
    local empty_table = {}
    
    -- This complex expression should not contaminate array_table register
    local result = type(empty_table) == "table" and #array_table == 5
    
    -- Validate array_table still contains correct values
    test("Table persistence in complex expression", 
         type(array_table) == "table" and array_table[1] == 1 and #array_table == 5)
    
    return result
end

-- Test 2: Register isolation during function calls with discarded results
local function test_call_register_isolation()
    local test_var = {a = 1, b = 2}
    
    -- Call function that discards results (C=1) - should not contaminate test_var
    local function dummy_func() return "ignored" end
    dummy_func()  -- This should not affect test_var register
    
    test("Register isolation after call with discarded results",
         type(test_var) == "table" and test_var.a == 1)
    
    return test_var
end

-- Test 3: Multi-value function call register management
local function test_multi_value_calls()
    local function multi_return() return 10, 20, 30 end
    
    local preserved_table = {x = "hello"}
    local a, b, c = multi_return()  -- Multi-value assignment
    
    -- Preserved table should not be contaminated by multi-value call
    test("Table preservation during multi-value calls",
         type(preserved_table) == "table" and preserved_table.x == "hello")
    
    test("Multi-value call results", a == 10 and b == 20 and c == 30)
    
    return preserved_table
end

-- Test 4: Complex expression chains with intermediate results
local function test_expression_chains()
    local data = {value = 42}
    
    -- Complex chain that creates many intermediate results
    local result = type(data) == "table" and 
                   data.value > 0 and 
                   #tostring(data.value) == 2 and
                   data.value == 42
    
    test("Data preservation in complex expression chain",
         type(data) == "table" and data.value == 42)
    
    return result
end

-- Test 5: Length operator register consistency
local function test_length_operations()
    local string_var = "hello world"
    local table_var = {10, 20, 30, 40}
    
    -- Multiple length operations should not cross-contaminate
    local string_len = #string_var
    local table_len = #table_var
    
    test("String length operation consistency",
         string_len == 11 and type(string_var) == "string")
    
    test("Table length operation consistency", 
         table_len == 4 and type(table_var) == "table")
    
    return string_len + table_len
end

-- Run comprehensive tests
test_table_persistence()
test_call_register_isolation()
test_multi_value_calls()
test_expression_chains()
test_length_operations()

print("\n===== Register Integrity Summary =====")
print("Tests Passed:", tests_passed)
print("Tests Failed:", tests_failed)
print("Total Tests:", tests_passed + tests_failed)

if tests_failed == 0 then
    print("✓ All register integrity tests PASSED")
    return true
else
    print("✗ Some register integrity tests FAILED")
    return false
end