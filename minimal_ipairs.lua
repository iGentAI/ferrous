local arr = {10, 20, 30}
local sum = 0

print("Starting ipairs test with table:", arr)
for i, v in ipairs(arr) do
  print("Index:", i, "Value:", v)
  sum = sum + v
end

print("Sum:", sum)  -- Should be 60
return sum
