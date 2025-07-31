-- Debug Equality Operator Test
-- This script tests equality comparison with a focus on number types
-- The issue we're debugging is how integer and float representations compare

print("=== Debug Equality Operator Test ===")

-- Create values for testing
local int_value = 1    -- Integer representation
local float_value = 1.0 -- Float representation

-- Test basic equality
print("\n1. Basic equality: Direct comparison of 1 and 1.0")
print(string.format("int_value = %s (type: %s)", int_value, type(int_value)))
print(string.format("float_value = %s (type: %s)", float_value, type(float_value)))
print("int_value == float_value:", int_value == float_value)
print("float_value == int_value:", float_value == int_value)
print("int_value == 1:", int_value == 1)
print("float_value == 1.0:", float_value == 1.0)

-- Test in different contexts
print("\n2. Equality in conditional expression")
if int_value == float_value then
    print("PASS: 1 == 1.0 is true in conditional")
else
    print("FAIL: 1 == 1.0 is false in conditional")
end

-- Direct assertion test (similar to the failing closure test)
print("\n3. Using assertion for equality")
local function test_assert()
    return assert(int_value == float_value, "1 should equal 1.0")
end

-- Use pcall to catch any assertion errors
local status, result = pcall(test_assert)
print("Assert test status:", status)
if not status then
    print("Assert error:", result)
end

-- Test with variables modified by functions (similar to closure issue)
print("\n4. Equality after function modification")
local function get_int()
    return 1  -- Returns integer 1
end

local function get_float() 
    return 1.0  -- Returns float 1.0
end

print(string.format("get_int() = %s", get_int()))
print(string.format("get_float() = %s", get_float()))
print("get_int() == get_float():", get_int() == get_float())
print("get_int() == 1:", get_int() == 1)
print("get_float() == 1.0:", get_float() == 1.0)

-- Test within a closure context (minimal reproduction of the issue)
print("\n5. Equality with closures and upvalues")
local function create_counter()
    local count = 0
    print(string.format("  count initialized to: %s (type: %s)", count, type(count)))
    
    return function()
        count = count + 1
        print(string.format("  count incremented to: %s (type: %s)", count, type(count)))
        print("  count == 1:", count == 1)
        print("  1 == count:", 1 == count)
        return count
    end
end

local counter = create_counter()
local value = counter()
print(string.format("counter() returned: %s (type: %s)", value, type(value)))
print("value == 1:", value == 1)
print("1 == value:", 1 == value)

local success, err = pcall(function()
    assert(value == 1, "Counter should return 1")
end)
print("Assert status:", success)
if not success then
    print("Assert error:", err)
end

print("\nTest complete")