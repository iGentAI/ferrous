-- =====================================
-- Comprehensive pairs Test with Edge Cases
-- =====================================

print("Starting comprehensive pairs test")

-- Test 1: Basic pairs functionality with a hash table
print("\n== Test 1: Basic pairs with hash table ==")
local hash_table = {
  name = "test",
  value = 42,
  flag = true,
  nested = {inner = "data"}
}

print("Hash table content:")
local keys = {}
local count = 0
for k, v in pairs(hash_table) do
  count = count + 1
  keys[count] = k
  print(string.format("  key: %s, value type: %s", tostring(k), type(v)))
end
print("Total keys: " .. count)
assert(count == 4, "Basic pairs test failed - expected 4 keys, got " .. count)

-- Test 2: Empty table
print("\n== Test 2: pairs with empty table ==")
local empty_table = {}
local empty_count = 0
print("Testing pairs on empty table")
for k, v in pairs(empty_table) do
  empty_count = empty_count + 1
  print("This should not execute")
end
print("Iterations: " .. empty_count)
assert(empty_count == 0, "Empty table test failed - should have 0 iterations, got " .. empty_count)

-- Test 3: Mixed array and hash
print("\n== Test 3: pairs with mixed array and hash ==")
local mixed_table = {10, 20, 30, name = "mixed", [true] = "boolean key"}
local mixed_count = 0
local found_keys = {string = false, number = false, boolean = false}

print("Mixed table content:")
for k, v in pairs(mixed_table) do
  mixed_count = mixed_count + 1
  print(string.format("  key type: %s, key: %s, value: %s", 
                     type(k), tostring(k), tostring(v)))
  
  -- Track the types of keys we find
  if type(k) == "string" then found_keys.string = true end
  if type(k) == "number" then found_keys.number = true end
  if type(k) == "boolean" then found_keys.boolean = true end
end
print("Total iterations: " .. mixed_count)

assert(mixed_count == 5, "Mixed table test failed - expected 5 iterations, got " .. mixed_count)
assert(found_keys.string, "Mixed table test failed - should find string keys")
assert(found_keys.number, "Mixed table test failed - should find number keys")
assert(found_keys.boolean, "Mixed table test failed - should find boolean keys")

-- Test 4: Table with nil values
print("\n== Test 4: pairs with nil values ==")
local nil_table = {a = 1, b = nil, c = 3}
nil_table.d = nil -- Another nil assignment
local nil_count = 0
local keys_with_values = {}

print("Table with nils content:")
for k, v in pairs(nil_table) do
  nil_count = nil_count + 1
  keys_with_values[k] = v
  print(string.format("  key: %s, value: %s", tostring(k), tostring(v)))
end
print("Total iterations: " .. nil_count)

-- pairs should only iterate over non-nil values
assert(nil_count == 2, "Nil values test failed - expected 2 iterations, got " .. nil_count)
assert(keys_with_values.a == 1, "Expected keys_with_values.a == 1")
assert(keys_with_values.c == 3, "Expected keys_with_values.c == 3")
assert(keys_with_values.b == nil, "Expected keys_with_values.b to be nil")
assert(keys_with_values.d == nil, "Expected keys_with_values.d to be nil")

-- Test 5: Modification during iteration
print("\n== Test 5: Modifying table during pairs iteration ==")
local mod_table = {a = 1, b = 2, c = 3, d = 4}
local mod_count = 0
local mod_removed = false
local mod_added = false

print("Table before modification:")
for k, v in pairs(mod_table) do
  print(string.format("  %s: %d", k, v))
end

print("Table during modification iteration:")
for k, v in pairs(mod_table) do
  mod_count = mod_count + 1
  print(string.format("  Processing: %s: %d", k, v))
  
  -- Remove an item we haven't reached yet
  if k == "a" and mod_table.d ~= nil then
    print("  Removing key 'd' during iteration")
    mod_table.d = nil
    mod_removed = true
  end
  
  -- Add a new item
  if k == "b" and mod_table.x == nil then
    print("  Adding key 'x' during iteration")
    mod_table.x = 99
    mod_added = true
  end
end

print("Final table state:")
for k, v in pairs(mod_table) do
  print(string.format("  %s: %d", k, v))
end

print("\nAll pairs tests passed!")
local result = {
  basic_test_passed = true,
  empty_table_test_passed = true,
  mixed_table_test_passed = true,
  nil_values_test_passed = true,
  modification_test_passed = true
}
return result