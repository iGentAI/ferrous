-- Super minimal for loop test to isolate the issue
print("Starting simple for loop test")
local sum = 0
for i = 1, 3 do
  print("Loop iteration: " .. i)
  sum = sum + i
end
print("Final sum: " .. sum)
return sum