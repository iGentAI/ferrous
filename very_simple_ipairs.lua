-- Absolute minimum ipairs test
local t = {10, 20, 30} -- Table directly in R(0)
local sum = 0

-- This will properly pass the table to ipairs thanks to Move instruction
local function add_values()
  for i, v in ipairs(t) do
    sum = sum + v
  end
end

add_values()
print("Sum:", sum)  -- Should be 60
return sum