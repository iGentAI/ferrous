-- Super minimal for loop test to isolate the issue
local i
print("Before loop, i =", i)
for i = 1, 5, 1 do
  print("Start of iteration, i =", i)
  print("i =", i)
end

return "For loop completed"