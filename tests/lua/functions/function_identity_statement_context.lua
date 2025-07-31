-- Function Identity Preservation in Statement Contexts Test
-- This test specifically targets the defect where functions called for side effects
-- in statement contexts lose their identity and become their return values

-- Test 1: Basic function identity preservation
local function test_basic() 
  return true 
end

-- Call function in statement context (should preserve function identity)
test_basic()  -- This call expects 0 results (statement context)

-- Second call should still work (function identity preserved)
local result = test_basic()  -- This call expects 1 result (expression context)

print("Basic identity test result:", result)
assert(result == true, "Function identity test failed")

-- Test 2: Multiple statement context calls
local function counter()
  counter.count = (counter.count or 0) + 1
  return counter.count
end

-- Multiple statement context calls
counter()  -- Statement context call 1
counter()  -- Statement context call 2
counter()  -- Statement context call 3

-- Expression context call should work
local final_count = counter()
print("Counter final value:", final_count)
assert(final_count == 4, "Counter function identity failed")

-- Test 3: Function with side effects
local side_effect_value = 0

local function side_effect()
  side_effect_value = side_effect_value + 10
  return "side_effect_executed"
end

-- Statement context call (for side effects only)
side_effect()

-- Verify side effect occurred
print("Side effect value:", side_effect_value)
assert(side_effect_value == 10, "Side effect did not occur")

-- Expression context call should still work  
local effect_result = side_effect()
print("Side effect return:", effect_result)
assert(effect_result == "side_effect_executed", "Side effect function call failed")

return "Function identity preservation test passed"