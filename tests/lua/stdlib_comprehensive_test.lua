-- Comprehensive Standard Library Test Script for Ferrous Lua VM
-- This test verifies the functionality of the Lua standard library implementation

-- Test counters
local tests_total = 0
local tests_passed = 0
local tests_failed = 0

-- Test utility functions
local function assertEquals(expected, actual, message)
    tests_total = tests_total + 1
    
    if expected == actual then
        tests_passed = tests_passed + 1
        print("PASS: " .. (message or "Unnamed test"))
        return true
    else
        tests_failed = tests_failed + 1
        print("FAIL: " .. (message or "Unnamed test") .. " - Expected " .. tostring(expected) .. ", got " .. tostring(actual))
        return false
    end
end

local function assertTrue(value, message)
    return assertEquals(true, value, message)
end

local function assertFalse(value, message)
    return assertEquals(false, value, message)
end

local function assertNil(value, message)
    return assertEquals(nil, value, message)
end

local function assertNotNil(value, message)
    tests_total = tests_total + 1
    
    if value ~= nil then
        tests_passed = tests_passed + 1
        print("PASS: " .. (message or "Unnamed test"))
        return true
    else
        tests_failed = tests_failed + 1
        print("FAIL: " .. (message or "Unnamed test") .. " - Expected non-nil value, got nil")
        return false
    end
end

local function printHeader(name)
    print("\n=== Testing " .. name .. " ===")
end

local function printSummary()
    print("\n=== Test Summary ===")
    print("Tests: " .. tests_total .. ", Passed: " .. tests_passed .. ", Failed: " .. tests_failed)
    
    if tests_failed == 0 then
        print("ALL TESTS PASSED!")
        return true
    else
        print(tests_failed .. " TESTS FAILED!")
        return false
    end
end

-- Begin tests
print("Starting Lua Standard Library Tests")

-- Test base functions
printHeader("Base Library")

-- Test type()
assertEquals("number", type(42), "type(number)")
assertEquals("string", type("hello"), "type(string)")
assertEquals("boolean", type(true), "type(boolean)")
assertEquals("table", type({}), "type(table)")
assertEquals("function", type(print), "type(function)")
assertEquals("nil", type(nil), "type(nil)")

-- Test tostring()
assertEquals("42", tostring(42), "tostring(number)")
assertEquals("hello", tostring("hello"), "tostring(string)")
assertEquals("true", tostring(true), "tostring(boolean)")
assertEquals("false", tostring(false), "tostring(false)")
assertEquals("nil", tostring(nil), "tostring(nil)")
assertTrue(tostring({}) ~= nil, "tostring(table) returns a value")
assertTrue(string.find(tostring({}), "table: ") == 1, "tostring(table) format")

-- Test tonumber()
assertEquals(42, tonumber("42"), "tonumber(string)")
assertEquals(42, tonumber(42), "tonumber(number)")
assertEquals(nil, tonumber("hello"), "tonumber(invalid string)")
assertEquals(42, tonumber("42.0"), "tonumber(decimal string)")
assertEquals(255, tonumber("FF", 16), "tonumber(hex string)")
assertEquals(15, tonumber("1111", 2), "tonumber(binary string)")

-- Test assert()
local status, result = pcall(function() return assert(true, "This should not error") end)
assertTrue(status, "assert with true condition doesn't throw")
assertEquals("test value", assert("test value", "This should not error"), "assert returns arguments")

status, result = pcall(function() assert(false, "Expected error") end)
assertFalse(status, "assert with false condition throws")
assertTrue(string.find(result, "Expected error") ~= nil, "assert error message")

-- Test error()
status, result = pcall(function() error("Test error") end)
assertFalse(status, "error throws")
assertTrue(string.find(result, "Test error") ~= nil, "error message")

-- Test select()
assertEquals(3, select("#", "a", "b", "c"), "select('#') counts arguments")
assertEquals("b", select(2, "a", "b", "c"), "select(n) returns nth argument")
assertEquals("b", (select(2, "a", "b", "c")), "select with parentheses")

-- Test pairs() and ipairs()
local t = {10, 20, 30, a = "alpha", b = "beta"}
local count = 0
for k, v in pairs(t) do
    count = count + 1
end
assertEquals(5, count, "pairs() iterates all table elements")

count = 0
for i, v in ipairs(t) do
    count = count + 1
    assertEquals(t[i], v, "ipairs() value matches index")
    assertEquals(i, count, "ipairs() uses sequential indices")
end
assertEquals(3, count, "ipairs() iterates array elements only")

-- Test metatable functions
local mt = {__index = {extra = "value"}}
local t = {}
assertEquals(t, setmetatable(t, mt), "setmetatable returns the table")
assertEquals(mt, getmetatable(t), "getmetatable returns the metatable")
assertEquals("value", t.extra, "__index metamethod works")

-- Test raw functions
local t = {10, 20, 30}
assertEquals(10, rawget(t, 1), "rawget gets values")
t[2] = nil
rawset(t, 2, "test")
assertEquals("test", t[2], "rawset sets values")
assertTrue(rawequal(t, t), "rawequal with same table")
assertFalse(rawequal(t, {}), "rawequal with different tables")
assertTrue(rawequal("hello", "hello"), "rawequal with same string")
assertFalse(rawequal("hello", "world"), "rawequal with different strings")

-- Test unpack()
local a, b, c = unpack({10, 20, 30})
assertEquals(10, a, "unpack first value")
assertEquals(20, b, "unpack second value")
assertEquals(30, c, "unpack third value")

-- Test pcall()
local function succeed() return "ok" end
local function fail() error("Expected error") end

status, result = pcall(succeed)
assertTrue(status, "pcall with successful function")
assertEquals("ok", result, "pcall returns function result")

status, result = pcall(fail)
assertFalse(status, "pcall with failing function")
assertTrue(string.find(result, "Expected error") ~= nil, "pcall error message")

-- If we have math library, test it
if math then
    printHeader("Math Library")
    
    -- Test math constants
    assertTrue(math.pi > 3.14 and math.pi < 3.15, "math.pi is approximately 3.14159...")
    assertEquals(1, math.cos(0), "math.cos(0) equals 1")
    assertEquals(0, math.sin(0), "math.sin(0) equals 0")
    assertEquals(0, math.tan(0), "math.tan(0) equals 0")
    
    -- Test basic math functions
    assertEquals(5, math.abs(-5), "math.abs works on negative numbers")
    assertEquals(5, math.abs(5), "math.abs works on positive numbers")
    assertEquals(2, math.ceil(1.1), "math.ceil rounds up")
    assertEquals(1, math.floor(1.9), "math.floor rounds down")
    assertEquals(4, math.floor(2^2), "math.floor of exact integer")
    
    -- Test math.max and math.min
    assertEquals(5, math.max(1, 3, 5, 2), "math.max finds maximum")
    assertEquals(1, math.min(1, 3, 5, 2), "math.min finds minimum")
    
    -- Test math.sqrt and math.pow
    assertEquals(2, math.sqrt(4), "math.sqrt works")
    assertEquals(8, math.pow(2, 3), "math.pow works")
    
    -- Test math.random
    local r1 = math.random()
    local r2 = math.random() 
    assertTrue(r1 >= 0 and r1 < 1, "math.random() in [0,1)")
    assertTrue(r2 >= 0 and r2 < 1, "math.random() in [0,1)")
    assertFalse(r1 == r2, "math.random() returns different values")
    
    -- Test math.randomseed
    math.randomseed(42)
    local r3 = math.random()
    math.randomseed(42)
    local r4 = math.random()
    assertEquals(r3, r4, "math.randomseed sets the seed")
end

-- If we have string library, test it
if string then
    printHeader("String Library")
    
    -- Test string.len
    assertEquals(5, string.len("hello"), "string.len counts characters")
    assertEquals(0, string.len(""), "string.len of empty string")
    
    -- Test string.sub
    assertEquals("el", string.sub("hello", 2, 3), "string.sub extracts substring")
    assertEquals("ello", string.sub("hello", 2), "string.sub with default end")
    assertEquals("hello", string.sub("hello", 1, 100), "string.sub with end beyond string")
    assertEquals("", string.sub("hello", 10, 12), "string.sub with start beyond string")
    assertEquals("o", string.sub("hello", -1), "string.sub with negative start")
    
    -- Test string.upper and string.lower
    assertEquals("HELLO", string.upper("Hello"), "string.upper converts to uppercase")
    assertEquals("hello", string.lower("Hello"), "string.lower converts to lowercase")
    
    -- Test string.char and string.byte
    assertEquals("ABC", string.char(65, 66, 67), "string.char creates string from bytes")
    assertEquals(65, string.byte("ABC", 1), "string.byte gets byte at position")
    local b1, b2, b3 = string.byte("ABC", 1, 3)
    assertEquals(65, b1, "string.byte first value")
    assertEquals(66, b2, "string.byte second value")
    assertEquals(67, b3, "string.byte third value")
    
    -- Test string.rep
    assertEquals("abcabc", string.rep("abc", 2), "string.rep repeats string")
    assertEquals("abc-abc", string.rep("abc", 2, "-"), "string.rep with separator")
    assertEquals("", string.rep("abc", 0), "string.rep with zero count")
    
    -- Test string.reverse
    assertEquals("olleh", string.reverse("hello"), "string.reverse works")
    assertEquals("", string.reverse(""), "string.reverse of empty string")
    
    -- Test string.format
    assertEquals("Hello, world!", string.format("Hello, %s!", "world"), "string.format with %s")
    assertEquals("Number: 42", string.format("Number: %d", 42.7), "string.format with %d")
    assertEquals("42.70", string.format("%.2f", 42.7), "string.format with %.2f")
    assertEquals("  42", string.format("%4d", 42), "string.format with %4d")
    assertEquals("42  ", string.format("%-4d", 42), "string.format with %-4d")
end

-- If we have table library, test it
if table then
    printHeader("Table Library")
    
    -- Test table.concat
    assertEquals("1-2-3", table.concat({1, 2, 3}, "-"), "table.concat with separator")
    assertEquals("123", table.concat({1, 2, 3}), "table.concat without separator")
    assertEquals("2-3", table.concat({1, 2, 3}, "-", 2), "table.concat with start index")
    assertEquals("2", table.concat({1, 2, 3}, "-", 2, 2), "table.concat with start and end index")
    assertEquals("", table.concat({1, 2, 3}, "-", 5, 6), "table.concat with out of range indices")
    
    -- Test table.insert
    local t = {1, 2, 3}
    table.insert(t, 4)
    assertEquals(4, #t, "table.insert increases length")
    assertEquals(4, t[4], "table.insert appends value")
    
    table.insert(t, 2, 10)
    assertEquals(5, #t, "table.insert at position increases length")
    assertEquals(10, t[2], "table.insert at position inserts value")
    assertEquals(1, t[1], "table.insert preserves values before")
    assertEquals(2, t[3], "table.insert shifts values after")
    
    -- Test table.remove
    local t = {1, 2, 3, 4}
    local v = table.remove(t)
    assertEquals(3, #t, "table.remove decreases length")
    assertEquals(4, v, "table.remove returns removed value")
    assertEquals(3, t[3], "table.remove removes last element")
    
    v = table.remove(t, 1)
    assertEquals(2, #t, "table.remove at position decreases length")
    assertEquals(1, v, "table.remove at position returns value")
    assertEquals(2, t[1], "table.remove shifts values")
    assertEquals(3, t[2], "table.remove preserves order")
    
    -- Test table.sort
    local t = {3, 1, 4, 2}
    table.sort(t)
    assertEquals(1, t[1], "table.sort first element")
    assertEquals(2, t[2], "table.sort second element")
    assertEquals(3, t[3], "table.sort third element")
    assertEquals(4, t[4], "table.sort fourth element")
    
    local t = {"d", "a", "c", "b"}
    table.sort(t)
    assertEquals("a", t[1], "table.sort strings first element")
    assertEquals("b", t[2], "table.sort strings second element")
    assertEquals("c", t[3], "table.sort strings third element")
    assertEquals("d", t[4], "table.sort strings fourth element")
    
    local t = {3, 1, 4, 2}
    table.sort(t, function(a, b) return a > b end)
    assertEquals(4, t[1], "table.sort with custom comparator first element")
    assertEquals(3, t[2], "table.sort with custom comparator second element")
    assertEquals(2, t[3], "table.sort with custom comparator third element")
    assertEquals(1, t[4], "table.sort with custom comparator fourth element")
end

-- Print summary
local all_passed = printSummary()

-- Return success status
return all_passed