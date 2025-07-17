-- Minimal for loop test to verify register protection
local sum = 0
for i = 1, 3 do
  sum = sum + i
end
print("Sum:", sum)
return sum