-- Test script to diagnose table return value issues
local result = {}  -- Create an empty table

-- Add a simple key-value pair
result.key = "value"

-- Add a numeric index
result[1] = 100

-- Try returning a simple value first to confirm the basics work
print("About to return a table...")

-- Now return the table
return result