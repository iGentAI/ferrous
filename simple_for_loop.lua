-- Minimal numeric for loop test
print("Testing for loop with single variable")
local sum = 0
for i = 1, 5 do
  print("Iteration: " .. i)
  sum = sum + i
end
print("Sum: " .. sum)

return { success = sum == 15 }