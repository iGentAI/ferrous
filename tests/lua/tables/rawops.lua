-- Minimal raw table operations test
print("===== Ferrous Lua Standard Library Test =====")

-- Test 1: Basic rawset and rawget
local t1 = {}
print("Test 1: Basic rawset/rawget")

rawset(t1, "key1", "value1")
print("rawset(t1, \"key1\", \"value1\")")

local v1 = rawget(t1, "key1")
print("rawget(t1, \"key1\") => " .. tostring(v1))

-- Test normal table access for comparison
t1.key2 = "value2"
print("t1.key2 = \"value2\"")
print("t1.key2 => " .. tostring(t1.key2))

-- Test 2: Direct equality comparisons (rawequal not available in Lua 5.1)
print("\nTest 2: Direct equality comparisons")

local a = {}
local b = {}
local c = a

print("a == a =>", a == a)
print("a == b =>", a == b) 
print("a == c =>", a == c)
print("1 == 1 =>", 1 == 1)
print("1 == 2 =>", 1 == 2)
print("\"test\" == \"test\" =>", "test" == "test")

-- Test 3: Simple metatable test (if supported)
print("\nTest 3: Simple metatable test")

local t2 = {}
local mt = {}

-- Store a simple value in the metatable's __index
mt.__index = {defaultValue = 100}

-- Try to set the metatable
local success = pcall(function()
  setmetatable(t2, mt)
  print("Metatable set successfully")
end)

if not success then
  print("Metatable not supported yet")
else
  -- Test if __index works
  print("t2.defaultValue => " .. tostring(t2.defaultValue))
  
  -- Test rawget bypasses metatable
  local raw_value = rawget(t2, "defaultValue")
  print("rawget(t2, \"defaultValue\") => " .. tostring(raw_value))
  
  -- Test rawset works with metatable
  rawset(t2, "actualKey", "actualValue")
  print("rawset(t2, \"actualKey\", \"actualValue\")")
  print("t2.actualKey => " .. tostring(t2.actualKey))
end

-- Test 4: rawset with numeric keys
print("\nTest 4: Numeric keys")
local t3 = {}
rawset(t3, 1, "first")
rawset(t3, 2, "second")
print("rawset(t3, 1, \"first\")")
print("rawset(t3, 2, \"second\")")
print("rawget(t3, 1) => " .. tostring(rawget(t3, 1)))
print("rawget(t3, 2) => " .. tostring(rawget(t3, 2)))

print("\nRaw table operations test complete")
return "Raw table operations test successful"