-- Generic For Loop Test (TFORLOOP)
-- Tests the generic for loop iteration protocol
-- Status: FAILING - Current implementation has issues with the iterator protocol

local t = {a = 1, b = 2, c = 3, d = 4}

-- Test pairs() iterator
local keys = {}
local values = 0

-- This should iterate over all table elements in any order
for k, v in pairs(t) do
  keys[k] = true
  values = values + v
end

print("Sum of values:", values)
print("Keys found:", keys.a, keys.b, keys.c, keys.d)

-- Test ipairs() iterator with array
local arr = {10, 20, 30, 40, 50}
local sum = 0

-- This should iterate over array indices in numeric order
for i, v in ipairs(arr) do
  print("Array element", i, "=", v)
  sum = sum + v
end

print("Sum of array:", sum)

-- Define our own iterator
local function my_iterator(t, index)
  local key
  if index == nil then
    key = "a"
  elseif index == "a" then
    key = "b"
  elseif index == "b" then
    key = "c"
  elseif index == "c" then
    key = nil
  end
  
  if key then
    return key, t[key]
  end
  return nil -- End iteration
end

-- Use our custom iterator
local sum2 = 0
for k, v in my_iterator, t, nil do
  print("Custom iterator:", k, v)
  sum2 = sum2 + v
end

print("Custom iterator sum:", sum2)

return values == 10 and sum == 150 and sum2 == 6