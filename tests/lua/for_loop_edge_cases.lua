-- =====================================
-- For Loop Edge Cases and Comprehensive Tests
-- =====================================

print("Starting for loop edge cases test")

-- ===== Numeric for loop tests =====
print("\n== Numeric For Loop Tests ==")

-- Test 1: Basic numeric for loop with positive step
print("\n- Test 1: Basic numeric for with positive step")
local sum = 0
for i = 1, 5, 1 do
  print(string.format("  i = %d", i))
  sum = sum + i
end
print("Sum: " .. sum)
assert(sum == 15, "Basic for loop test failed - expected sum 15, got " .. sum)

-- Test 2: Negative step
print("\n- Test 2: Numeric for with negative step")
local desc_values = {}
for i = 10, 1, -2 do
  desc_values[#desc_values + 1] = i
  print(string.format("  i = %d", i))
end
print("Values: " .. table.concat(desc_values, ", "))
assert(#desc_values == 5, "Negative step test failed - expected 5 iterations, got " .. #desc_values)
assert(desc_values[1] == 10 and desc_values[5] == 2, "Negative step test failed - wrong values")

-- Test 3: Fractional step
print("\n- Test 3: Numeric for with fractional step")
local frac_values = {}
for i = 1, 3, 0.5 do
  frac_values[#frac_values + 1] = i
  print(string.format("  i = %.1f", i))
end
print("Count: " .. #frac_values)
assert(#frac_values == 5, "Fractional step test failed - expected 5 iterations, got " .. #frac_values)

-- Test 4: Empty loop (start > end with positive step)
print("\n- Test 4: Empty numeric for loop")
local empty_count = 0
for i = 5, 1, 1 do
  empty_count = empty_count + 1
  print("This should not execute")
end
print("Iterations: " .. empty_count)
assert(empty_count == 0, "Empty loop test failed - expected 0 iterations, got " .. empty_count)

-- Test 5: Single iteration
print("\n- Test 5: Single iteration for loop")
local single_count = 0
for i = 7, 7, 1 do
  single_count = single_count + 1
  print(string.format("  i = %d", i))
end
print("Iterations: " .. single_count)
assert(single_count == 1, "Single iteration test failed - expected 1 iteration, got " .. single_count)

-- Test 6: Step 0 (should be an infinite loop but we'll limit it)
print("\n- Test 6: Zero step for loop (limiting to 5 iterations)")
local zero_step_count = 0
for i = 1, 10, 0 do
  zero_step_count = zero_step_count + 1
  print(string.format("  i = %d (iteration %d)", i, zero_step_count))
  if zero_step_count >= 5 then break end  -- Prevent actual infinite loop
end
print("Iterations: " .. zero_step_count)
assert(zero_step_count == 5, "Zero step test failed - expected 5 iterations, got " .. zero_step_count)

-- ===== Generic for loop with ipairs tests =====
print("\n== Generic For Loop with ipairs Tests ==")

-- Test 7: Basic ipairs
print("\n- Test 7: Basic ipairs")
local array = {10, 20, 30, 40, 50}
local ipairs_sum = 0
for i, v in ipairs(array) do
  print(string.format("  index %d: value %d", i, v))
  ipairs_sum = ipairs_sum + v
end
print("Sum: " .. ipairs_sum)
assert(ipairs_sum == 150, "Basic ipairs test failed - expected sum 150")

-- Test 8: ipairs with empty array
print("\n- Test 8: ipairs with empty array")
local empty_array = {}
local ipairs_empty_count = 0
for i, v in ipairs(empty_array) do
  ipairs_empty_count = ipairs_empty_count + 1
  print("This should not execute")
end
print("Iterations: " .. ipairs_empty_count)
assert(ipairs_empty_count == 0, "Empty ipairs test failed - expected 0 iterations, got " .. ipairs_empty_count)

-- Test 9: ipairs with sparse array (nil values)
print("\n- Test 9: ipairs with nil values")
local sparse = {10, 20, nil, 40, 50}
local ipairs_sparse_count = 0
local sparse_values = {}
for i, v in ipairs(sparse) do
  ipairs_sparse_count = ipairs_sparse_count + 1
  sparse_values[i] = v
  print(string.format("  index %d: value %s", i, tostring(v)))
end
print("Iterations: " .. ipairs_sparse_count)
-- ipairs should stop at the first nil
assert(ipairs_sparse_count == 2, "Sparse ipairs test failed - expected 2 iterations, got " .. ipairs_sparse_count)

-- Test 10: ipairs with large array (testing register window bounds)
print("\n- Test 10: ipairs with large array")
local large_array = {}
for i = 1, 50 do
  large_array[i] = i * 10
end
local large_ipairs_count = 0
local large_ipairs_sum = 0
for i, v in ipairs(large_array) do
  large_ipairs_count = large_ipairs_count + 1
  large_ipairs_sum = large_ipairs_sum + v
  -- Only print a few elements to avoid flooding output
  if i <= 3 or i >= 48 then
    print(string.format("  index %d: value %d", i, v))
  elseif i == 4 then
    print("  ... (skipping middle values) ...")
  end
end
print(string.format("Count: %d, Sum: %d", large_ipairs_count, large_ipairs_sum))
assert(large_ipairs_count == 50, "Large ipairs test failed - expected 50 iterations")
-- Sum should be 10+20+...+500 = 10*(1+2+...+50) = 10*1275 = 12750
assert(large_ipairs_sum == 12750, "Large ipairs test failed - wrong sum")

-- ===== Generic for loop with pairs tests =====
print("\n== Generic For Loop with pairs Tests ==")

-- Test 11: Basic pairs
print("\n- Test 11: Basic pairs")
local hash = {a = 10, b = 20, c = 30}
local pairs_sum = 0
print("Hash table content:")
for k, v in pairs(hash) do
  print(string.format("  key %s: value %d", k, v))
  pairs_sum = pairs_sum + v
end
print("Sum: " .. pairs_sum)
assert(pairs_sum == 60, "Basic pairs test failed - expected sum 60")

-- Test 12: pairs with complex keys
print("\n- Test 12: pairs with complex keys")
local complex = {}
complex[true] = "boolean"
complex[{}] = "table"  -- Note: this key won't be found again due to reference inequality
complex[42] = "number"
local complex_count = 0
print("Table with complex keys:")
for k, v in pairs(complex) do
  complex_count = complex_count + 1
  print(string.format("  key type %s, value %s", type(k), v))
end
print("Count: " .. complex_count)
assert(complex_count == 3, "Complex keys test failed - expected 3 iterations")

print("\nAll for loop tests passed!")
local result = {
  numeric_for_tests_passed = true,
  ipairs_tests_passed = true,
  pairs_tests_passed = true
}
return result