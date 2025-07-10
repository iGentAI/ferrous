-- =====================================
-- Comprehensive ipairs Test with Edge Cases
-- =====================================

print("Starting comprehensive ipairs test")

-- Test 1: Basic ipairs functionality with a simple array
print("\n== Test 1: Basic ipairs with sequential indices ==")
local basic_array = {10, 20, 30, 40, 50}
local total = 0
print("Array content:")
for i, v in ipairs(basic_array) do
  print(string.format("  index %d: value %d", i, v))
  total = total + v
end
print("Sum: " .. total)
assert(total == 150, "Basic ipairs test failed - expected sum 150, got " .. total)

-- Test 2: Empty array
print("\n== Test 2: ipairs with empty array ==")
local empty_array = {}
local count = 0
print("Testing ipairs on empty array")
for i, v in ipairs(empty_array) do
  count = count + 1
  print("This should not execute")
end
print("Iterations: " .. count)
assert(count == 0, "Empty array test failed - should have 0 iterations, got " .. count)

-- Test 3: Array with nil holes
print("\n== Test 3: ipairs with nil holes ==")
local sparse_array = {10, 20, nil, 40, 50}
local values = {}
print("Array content with nil at index 3:")
for i, v in ipairs(sparse_array) do
  print(string.format("  index %d: value %s", i, tostring(v)))
  values[i] = v
end
-- ipairs should stop at first nil
assert(#values == 2, "Sparse array test failed - should stop at first nil")
assert(values[1] == 10, "Expected values[1] == 10")
assert(values[2] == 20, "Expected values[2] == 20")
assert(values[3] == nil, "Expected values[3] == nil")

-- Test 4: Large array to test register window bounds
print("\n== Test 4: Large array test ==")
local large_array = {}
for i = 1, 20 do
  large_array[i] = i * 10
end

local large_sum = 0
print("Testing with 20 elements:")
for i, v in ipairs(large_array) do
  large_sum = large_sum + v
  -- Only print a few elements to avoid flooding the output
  if i <= 5 or i >= 16 then
    print(string.format("  index %d: value %d", i, v))
  elseif i == 6 then
    print("  ... skipping middle values ...")
  end
end
-- Sum should be 10+20+...+200 = 10*(1+2+...+20) = 10*210 = 2100
assert(large_sum == 2100, "Large array test failed - expected sum 2100, got " .. large_sum)

-- Test 5: Non-sequential indices
print("\n== Test 5: Non-sequential indices ==")
local non_seq = {[1] = "one", [2] = "two", [5] = "five", [7] = "seven"}
local indices = {}
print("Non-sequential array:")
for i, v in ipairs(non_seq) do
  print(string.format("  index %d: value %s", i, v))
  indices[#indices + 1] = i
end
-- ipairs should only iterate through sequential indices starting at 1
assert(#indices == 2, "Non-sequential indices test failed - expected 2 iterations, got " .. #indices)
assert(indices[1] == 1 and indices[2] == 2, "Expected indices [1,2], got different values")

print("\nAll ipairs tests passed!")
local result = {
  basic_test_passed = true,
  empty_array_test_passed = true,
  nil_hole_test_passed = true,
  large_array_test_passed = true,
  non_sequential_test_passed = true
}
return result