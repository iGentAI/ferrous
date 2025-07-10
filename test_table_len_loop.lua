-- Targeted test for table length in for loop

-- Create a table with initial values
local t = {10, 20, 30}
print("Initial table created")

-- Get the table length manually
local len = #t
print("Table length: " .. len)

-- Try to use table length in a for loop
for i=1, 1 do
  -- Simple test with just one iteration
  print("In loop iteration " .. i)
  
  -- Access t with explicit index
  local value = t[i]
  print("t[" .. i .. "] = " .. value)
end

-- Return the table and its length
return t, len
