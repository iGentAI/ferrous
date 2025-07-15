-- Test to distinguish between string literals and global variables

-- String literal usage
print("This is a string literal")

-- Using a variable
local x = 42
print(x) -- Should use x as a variable, not try to look up global "x"

-- Create a function that uses a string literal
local function test_string_literal()
  print("Inside a function - string literal")
  return true
end

test_string_literal()

return "Test completed"
