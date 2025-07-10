-- Test ipairs functionality with a simple array
local arr = {10, 20, 30, 40, 50}
local sum = 0

print("Testing ipairs with a simple array")
for i, v in ipairs(arr) do
  print(i, v)
  sum = sum + v
end

print("Sum is:", sum)
return sum  -- should be 150