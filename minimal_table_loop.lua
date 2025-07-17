-- Minimal test of for loops with table access

-- Create a simple table
local t = {10, 20, 30, 40, 50}

-- Print all elements directly (this works)
print("Direct access to table elements:")
print("t[1] = " .. t[1])
print("t[2] = " .. t[2])
print("t[3] = " .. t[3])

-- Try a basic for loop
print("\nBasic numeric for loop:")
for i = 1, 3 do
  print("i = " .. i)
end

-- Try a for loop with table access (accessing t[1] directly)
print("\nTable access inside for loop (fixed index):")
for i = 1, 3 do
  print("t[1] = " .. t[1])
end

print("Test completed.")
return true