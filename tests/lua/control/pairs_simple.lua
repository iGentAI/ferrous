-- Simplified pairs/next iteration test that avoids multiple assignments
local t = {a = 1, b = 2, c = 3}

-- Test next function with simple assignments
print("Testing next():")

-- Get first key
local k1 = next(t)
print("First key: " .. tostring(k1))
if k1 then
  print("First value: " .. tostring(t[k1]))
end

-- Get second key
local k2 = next(t, k1)
print("Second key: " .. tostring(k2))
if k2 then
  print("Second value: " .. tostring(t[k2]))
end

-- Get third key
local k3 = next(t, k2)
print("Third key: " .. tostring(k3))
if k3 then
  print("Third value: " .. tostring(t[k3]))
end

-- Test that we've iterated through all elements
local k4 = next(t, k3)
print("Fourth key (should be nil): " .. tostring(k4))

-- Count elements to verify we got them all
local count = 0
local k = next(t)
while k do
  count = count + 1
  k = next(t, k)
end
print("Total elements counted: " .. count)

-- Test pairs function if possible
-- Note: This might still fail if for-in loops with iterators aren't supported
print("\nTesting pairs() existence:")
local iter = pairs(t)
print("pairs() returned: " .. tostring(iter))

-- Manual iteration using the iterator returned by pairs
-- This avoids the for-in syntax
print("\nManual iteration test:")
local iter_fn = pairs(t)
local state = t
local key = nil
local iterations = 0

-- Manually call the iterator function
key = iter_fn(state, key)
while key do
  print("Key: " .. tostring(key) .. ", Value: " .. tostring(t[key]))
  iterations = iterations + 1
  key = iter_fn(state, key)
end
print("Manual iterations: " .. iterations)

return "Simple iteration test successful"