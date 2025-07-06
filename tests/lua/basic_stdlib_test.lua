-- Basic test for Lua standard library 
-- Avoids complex control structures and jumps

-- Test print function
print("Basic stdlib test")

-- Test type function
print("Type of nil:", type(nil))
print("Type of number:", type(42))
print("Type of string:", type("hello"))
print("Type of table:", type({}))

-- Test tostring function
print("String representation of number:", tostring(42))
print("String representation of bool:", tostring(true))
print("String representation of nil:", tostring(nil))

-- Test basic function calls
local function add(a, b)
  return a + b
end

print("Function result:", add(2, 3))

print("Basic tests completed successfully")