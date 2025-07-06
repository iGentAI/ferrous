-- Lua Standard Library Test Suite
-- This script systematically tests the standard library functions

local results = {}
local current_test = ""

-- Test helper function
local function test(name, fn)
    current_test = name
    results[name] = { status = "pass" }
    local success, result = pcall(fn)
    if not success then
        results[name].status = "fail"
        results[name].error = result
        print("❌ Test failed: " .. name .. " - " .. result)
    else
        print("✓ Test passed: " .. name)
    end
    return success
end

-- Basic value tests
test("basic_nil", function()
    assert(nil == nil)
end)

test("basic_boolean", function()
    assert(true ~= false)
    assert(not false == true)
    assert(not true == false)
end)

test("basic_number", function()
    assert(1 + 1 == 2)
    assert(5 - 3 == 2)
    assert(2 * 3 == 6)
    assert(6 / 3 == 2)
    assert(5 % 2 == 1)
    assert(2 ^ 3 == 8)
end)

-- Type function tests
test("type_function", function()
    assert(type(nil) == "nil")
    assert(type(true) == "boolean")
    assert(type(42) == "number")
    assert(type("hello") == "string")
    assert(type({}) == "table")
    assert(type(print) == "function")
    assert(type(type) == "function")
end)

-- Tostring function tests
test("tostring_function", function()
    assert(tostring(nil) == "nil")
    assert(tostring(true) == "true")
    assert(tostring(42) == "42")
    assert(tostring(3.14) == "3.14")
    assert(tostring("hello") == "hello")
end)

-- Tonumber function tests
test("tonumber_function", function()
    assert(tonumber("42") == 42)
    assert(tonumber("3.14") == 3.14)
    assert(tonumber("0xFF", 16) == 255)
    assert(tonumber("1010", 2) == 10)
    assert(tonumber("not a number") == nil)
end)

-- Table tests
test("table_create_and_access", function()
    local t = {a = 1, b = 2}
    assert(t.a == 1)
    assert(t.b == 2)
    assert(t.c == nil)
    
    t.c = 3
    assert(t.c == 3)
    
    t[1] = "one"
    t[2] = "two"
    assert(t[1] == "one")
    assert(t[2] == "two")
end)

-- Next function test
test("next_function", function()
    local t = {a = 1, b = 2, c = 3}
    local count = 0
    local sum = 0
    
    local k, v = next(t)
    while k do
        count = count + 1
        if type(v) == "number" then
            sum = sum + v
        end
        k, v = next(t, k)
    end
    
    assert(count == 3, "Should have 3 key-value pairs")
    assert(sum == 6, "Sum should be 6")
end)

-- Pairs function test
test("pairs_function", function()
    local t = {a = 1, b = 2, c = 3}
    local count = 0
    local sum = 0
    
    for k, v in pairs(t) do
        count = count + 1
        sum = sum + v
    end
    
    assert(count == 3, "Should have iterated 3 elements")
    assert(sum == 6, "Sum should be 6")
end)

-- Ipairs function test
test("ipairs_function", function()
    local t = {"one", "two", "three"}
    local count = 0
    
    for i, v in ipairs(t) do
        count = count + 1
        assert(i == count, "Index should match count")
    end
    
    assert(count == 3, "Should have iterated 3 elements")
end)

-- Metatable tests
test("metatables", function()
    local t = {}
    local mt = {}
    
    -- Test setting metatable
    setmetatable(t, mt)
    assert(getmetatable(t) == mt)
    
    -- Test __index metamethod
    mt.__index = {value = 42}
    assert(t.value == 42)
    
    -- Test clearing metatable
    setmetatable(t, nil)
    assert(getmetatable(t) == nil)
end)

-- Raw table access tests
test("raw_table_access", function()
    local t = {}
    local mt = {
        __index = {hidden = "found"},
        __newindex = function(table, key, value)
            rawset(table, key, value .. " modified")
        end
    }
    setmetatable(t, mt)
    
    -- Test normal access
    assert(t.hidden == "found")
    
    -- Test raw access
    assert(rawget(t, "hidden") == nil)
    
    -- Test metamethod-modified assignment
    t.test = "value"
    assert(t.test == "value modified")
    
    -- Test raw set
    rawset(t, "direct", "unmodified")
    assert(t.direct == "unmodified")
end)

-- Unpack test
test("unpack_function", function()
    local t = {10, 20, 30, 40}
    
    -- Basic unpack
    local a, b, c = unpack(t)
    assert(a == 10 and b == 20 and c == 30)
    
    -- Unpack with start index
    local x, y = unpack(t, 2)
    assert(x == 20 and y == 30)
    
    -- Unpack with start and end index
    local p, q = unpack(t, 3, 4)
    assert(p == 30 and q == 40)
end)

-- Select test
test("select_function", function()
    -- Count
    assert(select("#", "a", "b", "c") == 3)
    
    -- Element at index
    assert(select(2, "a", "b", "c") == "b")
    
    -- Multiple elements from index
    local a, b = select(2, "a", "b", "c")
    assert(a == "b" and b == "c")
end)

-- Function tests
test("function_basics", function()
    local function add(a, b)
        return a + b
    end
    
    assert(type(add) == "function")
    assert(add(2, 3) == 5)
    
    -- Test function as variable
    local fn = add
    assert(fn(5, 5) == 10)
    
    -- Test anonymous function
    local mult = function(a, b) return a * b end
    assert(mult(3, 4) == 12)
end)

-- Print summary of results
print("\nTest Summary:")
local passed = 0
local failed = 0

for name, result in pairs(results) do
    if result.status == "pass" then
        passed = passed + 1
    else
        failed = failed + 1
    end
end

print("Passed: " .. passed .. ", Failed: " .. failed .. ", Total: " .. (passed + failed))

-- Return the results
return {
    passed = passed,
    failed = failed,
    details = results
}