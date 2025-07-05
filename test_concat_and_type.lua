-- Test script to isolate concatenation and type function bugs

-- First test the type function with different data types
local num_type = type(42)
print("Type of 42 is: " .. num_type)

local str_type = type("hello")
print("Type of 'hello' is: " .. str_type)

local bool_type = type(true)
print("Type of true is: " .. bool_type)

-- Test concatenation with different operand types
local a = "Hello, "
local b = "World!"
local c = a .. b
print(c)

local num = 42
local str = "The answer is: "
local result = str .. num
print(result)

-- Return a value to verify the final result
return "Test completed successfully"