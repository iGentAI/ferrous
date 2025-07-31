-- Metamethod Variable Initialization Test
-- This test specifically targets the issue where constants become Nil
-- in metamethod execution contexts due to stale prototype references

-- Test 1: Basic metamethod arithmetic with constants
local t1 = {value = 5}
local t2 = {value = 3}

local mt = {}
mt.__add = function(a, b)
  -- This arithmetic should access constants correctly (Number(1.0), not Nil)
  local result = a.value + b.value + 1  -- The constant 1 should not become Nil
  print("Metamethod arithmetic:", a.value, "+", b.value, "+ 1 =", result)
  return {value = result}
end

setmetatable(t1, mt)
setmetatable(t2, mt)

-- Trigger metamethod that involves constant access
local sum = t1 + t2
print("Basic metamethod result:", sum.value)
assert(sum.value == 9, "Basic metamethod arithmetic failed")

-- Test 2: Complex metamethod with multiple constants
local complex_mt = {}
complex_mt.__mul = function(a, b)
  -- Multiple constants that should not become Nil
  local factor1 = 2
  local factor2 = 3  
  local base = a.value * b.value
  local result = base * factor1 + factor2  -- Multiple constant access
  print("Complex metamethod:", base, "*", factor1, "+", factor2, "=", result)
  return {value = result}
end

local t3 = {value = 4}
local t4 = {value = 5}
setmetatable(t3, complex_mt)

local product = t3 * t4
print("Complex metamethod result:", product.value)
assert(product.value == 43, "Complex metamethod arithmetic failed") -- 4*5*2+3 = 43

-- Test 3: Nested metamethod calls  
local nested_mt = {}
nested_mt.__add = function(a, b)
  local temp = {value = a.value + 10}  -- Constant 10 access
  local temp2 = {value = b.value + 20} -- Constant 20 access
  return temp + temp2  -- Recursive metamethod call
end

local t5 = {value = 1}
local t6 = {value = 2}
setmetatable(t5, nested_mt)
setmetatable(t6, nested_mt)

local nested_result = t5 + t6
print("Nested metamethod result:", nested_result.value)
assert(nested_result.value == 33, "Nested metamethod failed") -- (1+10) + (2+20) = 33

return "Metamethod variable initialization test passed"