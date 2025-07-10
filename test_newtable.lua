-- Test for the NewTable opcode

-- Create an empty table
local t1 = {}

-- Create a table with initial values
local t2 = {10, 20, 30, key1 = "value1", key2 = "value2"}

-- Verify table operation by adding elements
t1[1] = "first"
t1[2] = "second"
t1["key"] = "value"

-- Use table.insert to add elements (tests metamethods)
for i=1,5 do
  t2[#t2 + 1] = i * 10
end

-- Return both tables to verify
return t1, t2
