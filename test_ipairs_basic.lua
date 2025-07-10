-- Very basic ipairs test
local t = {10, 20, 30}
local sum = 0

for i, v in ipairs(t) do
  print("Loop:", i, v)
  sum = sum + v
end

print("Final sum:", sum)
return sum  -- should be 60