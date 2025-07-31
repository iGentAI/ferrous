-- Simplified Generic For Loop Test (TFORLOOP)
-- Tests the generic for loop iteration protocol without multiple iterator expressions
-- Status: Compatible with current compiler limitations

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

-- Define a custom iterator that works like pairs/ipairs
-- This avoids the multiple iterator expression syntax
local function my_pairs(t)
  -- Define the actual iterator function
  local function my_iterator(tbl, index)
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
      return key, tbl[key]
    end
    return nil -- End iteration
  end
  
  -- Return the triplet: iterator function, state, initial control value
  return my_iterator, t, nil
end

-- Use our custom iterator wrapped in a function
local sum2 = 0
for k, v in my_pairs(t) do
  print("Custom pairs:", k, v)
  sum2 = sum2 + v
end

print("Custom iterator sum:", sum2)

-- Test with a simpler numeric iterator
local function count_to(n)
  local function iter(max, current)
    if current < max then
      return current + 1, current + 1
    end
  end
  return iter, n, 0
end

local count_sum = 0
for i, v in count_to(5) do
  print("Count:", i)
  count_sum = count_sum + v
end

print("Count sum:", count_sum)

-- All tests should pass
assert(values == 10, "pairs() test failed")
assert(sum == 150, "ipairs() test failed") 
assert(sum2 == 6, "custom pairs test failed")
assert(count_sum == 15, "count iterator test failed")

print("All tests passed!")
return true