-- Simple debug script for for loops and table access
local t = {10, 20, 30, 40, 50} -- Create test table
print("Step 1: Created table with " .. #t .. " elements")

-- Access individual elements outside a loop (works)
print("Step 2: Direct table access:")
print("t[1] = " .. t[1])

-- Simple for loop without table access
print("\nStep 3: Basic for loop:")
for i = 1, 3 do
  print("i = " .. i)
end

-- For loop with fixed table access
print("\nStep 4: For loop with fixed table access:")
for i = 1, 2 do
  print("Fixed: t[1] = " .. t[1])
end
