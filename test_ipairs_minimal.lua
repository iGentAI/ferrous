-- Very minimal ipairs test to validate the VM implementation
local arr = {10, 20, 30}
local sum = 0

for i, v in ipairs(arr) do
  sum = sum + v
end

print("Sum: " .. sum)
return sum  -- Should be 60