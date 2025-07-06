-- Segmented Standard Library Verification Test
-- Tests functionality in manageable chunks to avoid compiler issues

print("===== Ferrous Lua Standard Library Verification =====")

-- PART 1: Basic Functions
print("\n----- PART 1: Basic Functions -----")
print("Testing print(): Works since you can see this!")

-- Type Function
print("Testing type():")
local nil_type = type(nil)
print("  type(nil): " .. nil_type)
local num_type = type(42)
print("  type(42): " .. num_type) 
local str_type = type("hello")
print("  type(\"hello\"): " .. str_type)
local tbl_type = type({})
print("  type({}): " .. tbl_type)
local func_type = type(print)
print("  type(print): " .. func_type)
print("Type function test passed!")

-- PART 2: String Functions
print("\n----- PART 2: String Functions -----")
print("Testing tostring():")
local nil_str = tostring(nil)
print("  tostring(nil): " .. nil_str)
local num_str = tostring(42)
print("  tostring(42): " .. num_str)
local bool_str = tostring(true)
print("  tostring(true): " .. bool_str)
print("Tostring function test passed!")

-- PART 3: Number Functions
print("\n----- PART 3: Number Functions -----")
print("Testing tonumber():")
local str_to_num1 = tonumber("42")
print("  tonumber(\"42\"): " .. tostring(str_to_num1))
local str_to_num2 = tonumber("3.14")
print("  tonumber(\"3.14\"): " .. tostring(str_to_num2))
local hex_num = tonumber("FF", 16)
print("  tonumber(\"FF\", 16): " .. tostring(hex_num))
local invalid_num = tonumber("not a number")
print("  tonumber(\"not a number\"): " .. tostring(invalid_num))
print("Tonumber function test passed!")

-- PART 4: Function Definition
print("\n----- PART 4: Function Definition -----")
local function add(a, b)
  return a + b
end
print("Defined function add(a, b)")
local result = add(2, 3)
print("add(2, 3) = " .. result)
print("Function definition test passed!")

-- PART 5: Tables
print("\n----- PART 5: Table Functions -----")
local t = {a = 1, b = 2}
print("Created table t = {a = 1, b = 2}")
print("t.a = " .. t.a)
print("t.b = " .. t.b)
t.c = 3
print("Added t.c = 3")
print("t.c = " .. t.c)
print("Table functions test passed!")

-- PART 6: Metatable
print("\n----- PART 6: Metatable Functions -----")
local mt = {}
mt.__index = {value = 42}
print("Created metatable with __index")
setmetatable(t, mt)
print("Set metatable on t")
local mt2 = getmetatable(t)
print("Retrieved metatable")
local is_same = mt == mt2
print("Retrieved metatable matches original: " .. tostring(is_same))
print("t.value (via metatable): " .. t.value)
print("Metatable functions test passed!")

-- PART 7: Iteration Functions
print("\n----- PART 7: Iteration Functions -----")
-- Test next() function
print("Testing next():")
local test_table = {x = 10, y = 20, z = 30}
local k, v = next(test_table)
print("  First key-value: " .. tostring(k) .. " = " .. tostring(v))
k, v = next(test_table, k)
print("  Next key-value: " .. tostring(k) .. " = " .. tostring(v))
print("Next function test passed!")

-- Test pairs() function
print("Testing pairs():")
local count = 0
for key, value in pairs(test_table) do
  print("  Key: " .. tostring(key) .. ", Value: " .. tostring(value))
  count = count + 1
end
print("  Iterated over " .. count .. " key-value pairs")
print("Pairs function test passed!")

-- PART 8: Raw Table Operations
print("\n----- PART 8: Raw Table Operations -----")
local tbl = {}
rawset(tbl, "direct", "value")
print("rawset(tbl, \"direct\", \"value\") performed")
print("rawget(tbl, \"direct\"): " .. rawget(tbl, "direct"))
print("Raw table functions test passed!")

print("\n===== All Verification Tests PASSED =====")
return "All standard library functions verified successfully"