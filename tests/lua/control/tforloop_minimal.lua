-- Minimal Generic For Loop Test (TFORLOOP)
-- Tests only the basic pairs/ipairs functionality
-- Status: Minimal test for TFORLOOP implementation

-- Test 1: pairs() with a simple table
local t = {a = 10, b = 20}
local sum1 = 0

for k, v in pairs(t) do
  print("pairs:", k, "=", v)
  sum1 = sum1 + v
end

assert(sum1 == 30, "pairs() sum should be 30")

-- Test 2: ipairs() with an array
local arr = {100, 200, 300}
local sum2 = 0

for i, v in ipairs(arr) do
  print("ipairs:", i, "=", v)
  sum2 = sum2 + v
end

assert(sum2 == 600, "ipairs() sum should be 600")

print("TFORLOOP minimal test passed!")
return true