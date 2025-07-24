-- Iterator State Management Test
-- This test specifically targets iterator state corruption in generic for loops
-- and validates proper TFORLOOP/TFORCALL implementation

-- Test 1: Basic pairs iteration with state validation
local test_table = {a = 1, b = 2, c = 3}
local keys_found = {}
local values_sum = 0

-- This should not corrupt iterator state
for key, value in pairs(test_table) do
  print("Pairs iteration:", key, "=", value)
  keys_found[key] = true
  values_sum = values_sum + value
end

-- Validate all keys were found
assert(keys_found.a and keys_found.b and keys_found.c, "Not all keys found in pairs iteration")
assert(values_sum == 6, "Values sum incorrect in pairs iteration")

-- Test 2: Nested iteration state management  
local outer_table = {x = {1, 2}, y = {3, 4}}
local total_sum = 0

for outer_key, inner_table in pairs(outer_table) do
  print("Outer key:", outer_key)
  for inner_index, inner_value in pairs(inner_table) do
    print("  Inner:", inner_index, "=", inner_value)
    total_sum = total_sum + inner_value
  end
end

assert(total_sum == 10, "Nested iteration state management failed")

-- Test 3: Custom iterator with proper state handling
local function custom_iterator(t, key)
  local next_key
  if key == nil then
    next_key = "first"
  elseif key == "first" then
    next_key = "second"
  elseif key == "second" then  
    next_key = "third"
  else
    next_key = nil
  end
  
  if next_key then
    return next_key, t[next_key]
  else
    return nil
  end
end

local function custom_pairs(t)
  return custom_iterator, t, nil
end

local custom_table = {first = "A", second = "B", third = "C"}
local custom_results = {}

for key, value in custom_pairs(custom_table) do
  print("Custom iteration:", key, "=", value)
  custom_results[key] = value
end

assert(custom_results.first == "A", "Custom iterator state failed for first")
assert(custom_results.second == "B", "Custom iterator state failed for second") 
assert(custom_results.third == "C", "Custom iterator state failed for third")

-- Test 4: Iterator with early termination (state cleanup)
local early_table = {a = 1, b = 2, c = 3, d = 4, e = 5}
local early_count = 0

for key, value in pairs(early_table) do
  early_count = early_count + 1
  print("Early iteration:", key, "=", value)
  if early_count >= 3 then
    break  -- Early termination should not corrupt state
  end
end

-- Subsequent iteration should work correctly
local remaining_count = 0
for key, value in pairs(early_table) do
  remaining_count = remaining_count + 1
end

assert(remaining_count == 5, "Iterator state corrupted after early termination")

return "Iterator state management test passed"