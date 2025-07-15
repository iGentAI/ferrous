-- Test for TFORLOOP implementation
-- This tests the generic for loop iterator protocol

print("Testing generic for loop with ipairs")
local arr = {10, 20, 30, 40, 50}
local sum = 0

print("Using ipairs:")
for i, v in ipairs(arr) do
  print(string.format("%d: %d", i, v))
  sum = sum + v
end

print("Sum using ipairs:", sum)
local sum1 = sum

-- Reset sum for pairs test
sum = 0

print("\nTesting generic for loop with pairs")
local obj = {a=5, b=10, c=15, d=20}

print("Using pairs:")
for k, v in pairs(obj) do
  print(string.format("%s: %d", k, v))
  sum = sum + v
end

print("Sum using pairs:", sum)
local sum2 = sum

-- Test using next directly
sum = 0
print("\nTesting next function directly:")
local k = nil
while true do
  k, v = next(obj, k)
  if k == nil then break end
  print(string.format("%s: %d", k, v))
  sum = sum + v
end

print("Sum using next:", sum)
local sum3 = sum

-- Return all sums to validate
return sum1, sum2, sum3
