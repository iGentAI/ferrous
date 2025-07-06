-- Test basic type function and function calls
print("Testing type function:")
print("  type(nil) =", type(nil))
print("  type(42) =", type(42))
print("  type('hello') =", type("hello"))

print("Testing function definition:")
local function add(a, b)
  return a + b
end

print("  add(2, 3) =", add(2, 3))
return "Success"
