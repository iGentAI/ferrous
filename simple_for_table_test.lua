-- Simple test for numeric for loop with direct table access
print("Testing simple for loop with table access")

-- Create a basic table
local t = {10, 20, 30, 40, 50}
print("Table created with " .. #t .. " elements")

-- Print each element in a for loop
print("\nAccessing table elements in a for loop:")
local sum = 0
for i = 1, 5 do
  -- Print each element directly with fixed indices
  local value = t[i]
  print("Element " .. i .. " = " .. value)
  sum = sum + value
end

print("\nTotal sum: " .. sum)

-- Verify the sum is correct
assert(sum == 150, "Sum should be 150")

print("\nTest completed successfully!")
return {success = true}