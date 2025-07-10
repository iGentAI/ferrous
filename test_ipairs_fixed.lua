-- Simple ipairs test with debug output
local t = {10, 20, 30}
local sum = 0

print("Starting ipairs iteration")
for i, v in ipairs(t) do
  print("Index:", i, "Value:", v)
  sum = sum + v
end
print("Sum:", sum)
return sum  -- Should be 60