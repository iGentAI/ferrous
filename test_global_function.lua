-- Test global function definitions
print("Testing global functions...")

-- Define a simple global function
function add(a, b)
  return a + b
end

-- Define a method
math.custom = {}
function math.custom:double(x)
  return x * 2
end

-- Test the functions
print("add(5, 10) =", add(5, 10))
print("math.custom:double(7) =", math.custom:double(7))

-- Return success if both work
return add(5, 10) == 15 and math.custom:double(7) == 14
