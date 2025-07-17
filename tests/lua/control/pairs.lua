-- Minimal pairs/next iteration test
local t = {a = 1, b = 2, c = 3}

-- Test next function directly
print("Testing next():")
local k, v = next(t)
print("First key-value: " .. tostring(k) .. " = " .. tostring(v))
k, v = next(t, k)
print("Second key-value: " .. tostring(k) .. " = " .. tostring(v))

-- Test pairs function
print("\nTesting pairs():")
local count = 0
for k, v in pairs(t) do
  print("Pair: " .. tostring(k) .. " = " .. tostring(v))
  count = count + 1
end
print("Total pairs: " .. count)

return "Iteration functions test successful"