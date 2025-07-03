-- Test Lua Standard Library Functions

-- Test print function
print("Testing standard library functions")
print("Multiple", "arguments", 123, true, false, nil)

-- Test type function
assert(type(nil) == "nil")
assert(type(true) == "boolean")
assert(type(42) == "number")
assert(type("hello") == "string")
assert(type({}) == "table")
assert(type(function() end) == "function")
assert(type(print) == "function")

-- Test tostring function
assert(tostring(nil) == "nil")
assert(tostring(true) == "true")
assert(tostring(false) == "false")
assert(tostring(42) == "42")
assert(tostring(3.14) == "3.14")
assert(tostring("hello") == "hello")

-- Test tonumber function
assert(tonumber("42") == 42)
assert(tonumber("3.14") == 3.14)
assert(tonumber("  42  ") == 42)
assert(tonumber("not a number") == nil)
assert(tonumber("ff", 16) == 255)
assert(tonumber("1010", 2) == 10)

-- Test assert function
assert(true)
assert(1)
assert("string")
assert({})

local ok = pcall(function() assert(false) end)
assert(not ok)

local ok, msg = pcall(function() assert(false, "custom error") end)
assert(not ok)
assert(msg:find("custom error"))

-- Test select function
assert(select("#", 1, 2, 3, 4, 5) == 5)
assert(select(2, "a", "b", "c", "d") == "b")
local a, b = select(3, 1, 2, 3, 4, 5)
assert(a == 3 and b == 4)

-- Test pairs and ipairs
local t = {10, 20, 30, x = "foo", y = "bar"}

-- Test ipairs (numeric indices)
local sum = 0
local count = 0
for i, v in ipairs(t) do
    assert(type(i) == "number")
    assert(i == count + 1)
    sum = sum + v
    count = count + 1
end
assert(count == 3)
assert(sum == 60)

-- Test pairs (all key-value pairs)
local keys = {}
local values = {}
for k, v in pairs(t) do
    table.insert(keys, k)
    table.insert(values, v)
end
assert(#keys >= 5) -- At least 1, 2, 3, x, y

-- Test next function
local k, v = next(t)
assert(k ~= nil)
assert(v ~= nil)

-- Test metatable functions
local mt = {}
local t1 = {}
assert(getmetatable(t1) == nil)
assert(setmetatable(t1, mt) == t1)
assert(getmetatable(t1) == mt)
assert(setmetatable(t1, nil) == t1)
assert(getmetatable(t1) == nil)

-- Test raw functions
local t2 = {}
local mt2 = {
    __index = {default = "value"},
    __newindex = function(t, k, v)
        rawset(t, k, v .. "_modified")
    end
}
setmetatable(t2, mt2)

-- Test rawget
assert(rawget(t2, "default") == nil)
assert(t2.default == "value") -- Uses __index

-- Test rawset
rawset(t2, "test", "direct")
assert(rawget(t2, "test") == "direct")

-- Test rawequal
local a = {1}
local b = {1}
assert(not rawequal(a, b)) -- Different tables
assert(rawequal(a, a)) -- Same table
assert(rawequal(1, 1))
assert(rawequal("hello", "hello"))

-- Test unpack
local arr = {10, 20, 30, 40}
local a, b, c = unpack(arr)
assert(a == 10 and b == 20 and c == 30)

local x, y = unpack(arr, 2, 3)
assert(x == 20 and y == 30)

print("All standard library tests passed!")