-- Minimal test case for validating TFORLOOP functionality
print("Starting minimal TFORLOOP test")

-- Create a simple array
local t = {10, 20, 30}

-- Define a simple integer-based iterator
local function int_iter(state, i)
    i = i or 0
    i = i + 1
    if i <= #state then
        return i, state[i]
    end
    return nil
end

-- Test 1: Basic iteration
print("\nTest 1: Basic iteration")
local sum = 0
for i, v in int_iter, t, nil do
    print(string.format("  Iteration %d: value %d", i, v))
    sum = sum + v
end
print(string.format("  Sum: %d", sum))
assert(sum == 60, "Sum should be 60")

-- Test 2: Empty table iteration
print("\nTest 2: Empty table iteration")
local count = 0
for i, v in int_iter, {}, nil do
    count = count + 1
end
assert(count == 0, "Empty table should have no iterations")
print("  No iterations (correct)")

-- Test 3: Single element
print("\nTest 3: Single element")
local val
for i, v in int_iter, {42}, nil do
    val = v
end
assert(val == 42, "Should extract single value 42")
print("  Single value: 42 (correct)")

-- Test 4: Using only first loop variable
print("\nTest 4: Using only first variable")
local keys = {}
for k in int_iter, t, nil do
    table.insert(keys, k)
end
assert(#keys == 3, "Should have 3 keys")
assert(keys[1] == 1 and keys[2] == 2 and keys[3] == 3, "Keys should be 1,2,3")
print("  Iterator with one variable works correctly")

print("\nAll tests passed successfully!")
return "success"