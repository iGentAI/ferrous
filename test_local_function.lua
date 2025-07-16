-- Test local function declaration
print("Testing local function...")

local function add(a, b)
  return a + b
end

print("Result of add(5, 10):", add(5, 10))

return add(5, 10) == 15
