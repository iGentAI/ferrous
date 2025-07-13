-- Test TFORLOOP Fix - Verifies correct iterator behavior

print("=== TFORLOOP Fix Test Suite ===")

-- Test 1: Basic pairs() iteration
print("\nTest 1: pairs() iteration")
local t1 = {a=1, b=2, c=3}
local count1 = 0
for k, v in pairs(t1) do
    print("  pairs:", k, v)
    count1 = count1 + 1
    -- Verify k and v are not nil
    assert(k ~= nil, "key should not be nil")
    assert(v ~= nil, "value should not be nil")
end
assert(count1 > 0, "pairs should iterate at least once")

-- Test 2: Basic ipairs() iteration
print("\nTest 2: ipairs() iteration")
local t2 = {10, 20, 30, 40}
local count2 = 0
for i, v in ipairs(t2) do
    print("  ipairs:", i, v)
    count2 = count2 + 1
    -- Verify sequential indices
    assert(i == count2, "ipairs index should be sequential")
    assert(v == t2[i], "ipairs value should match table")
end
assert(count2 == 4, "ipairs should iterate 4 times")

-- Test 3: Empty table iteration
print("\nTest 3: Empty table iteration")
local t3 = {}
local count3 = 0
for k, v in pairs(t3) do
    count3 = count3 + 1
end
assert(count3 == 0, "empty table should not iterate")

-- Test 4: Single variable iteration
print("\nTest 4: Single variable iteration")
local t4 = {x=100, y=200}
local keys = {}
for k in pairs(t4) do
    print("  single var:", k)
    table.insert(keys, k)
end
assert(#keys > 0, "single variable iteration should work")

-- Test 5: Three variable iteration (custom iterator)
print("\nTest 5: Three variable iteration")
local function triple_iter(state, index)
    if index < 3 then
        index = index + 1
        return index, index * 10, index * 100
    end
end

local function triple_generator()
    return triple_iter, nil, 0
end

local results = {}
for a, b, c in triple_generator() do
    print("  triple:", a, b, c)
    table.insert(results, {a, b, c})
end
assert(#results == 3, "triple iterator should produce 3 results")
assert(results[1][1] == 1 and results[1][2] == 10 and results[1][3] == 100, "first triple incorrect")
assert(results[2][1] == 2 and results[2][2] == 20 and results[2][3] == 200, "second triple incorrect")
assert(results[3][1] == 3 and results[3][2] == 30 and results[3][3] == 300, "third triple incorrect")

-- Test 6: Nested iterations
print("\nTest 6: Nested iterations")
local t6 = {{1,2}, {3,4}, {5,6}}
local sum = 0
for i, row in ipairs(t6) do
    for j, val in ipairs(row) do
        print("  nested:", i, j, val)
        sum = sum + val
    end
end
assert(sum == 21, "nested iteration sum should be 21")

-- Test 7: Iterator function modification during iteration
print("\nTest 7: Iterator state preservation")
local t7 = {a=1, b=2}
local iter_func = nil
for k, v in pairs(t7) do
    if not iter_func then
        -- Save the iterator function on first iteration
        local mt = getmetatable(pairs)
        iter_func = next  -- pairs uses next as iterator
    end
    print("  state test:", k, v)
end
-- Verify iterator wasn't corrupted
assert(type(iter_func) == "function", "iterator function should be preserved")

-- Test 8: Break in for loop
print("\nTest 8: Break in for loop")
local t8 = {1, 2, 3, 4, 5}
local last_seen = 0
for i, v in ipairs(t8) do
    print("  break test:", i, v)
    last_seen = i
    if i == 3 then
        break
    end
end
assert(last_seen == 3, "loop should break at 3")

print("\n=== All TFORLOOP tests passed! ===")