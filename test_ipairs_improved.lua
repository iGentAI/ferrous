-- Test ipairs functionality with a simple array
local arr = {10, 20, 30, 40, 50}
local sum = 0

print("Testing ipairs with a simple array")
print("Array elements:")
for i, v in ipairs(arr) do
  -- Print each element and its index
  print(string.format("  %d: %d", i, v))
  sum = sum + v
  
  -- Print info about the current sum
  print(string.format("  Current sum: %d", sum))
end

print(string.format("Final sum: %d", sum))
return sum  -- should be 150