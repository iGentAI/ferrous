-- Test to check parameter passing
local function f(a, b)
  -- Print the parameters
  print("a =", a, "type:", type(a))
  print("b =", b, "type:", type(b))
  return a + b
end

-- Call the function
local result = f(3, 4)
print("Result:", result)

return result == 7
