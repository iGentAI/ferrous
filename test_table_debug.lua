-- Simple table return test with debug prints

-- Create a table with specific content for easier debugging
local t = {}
t.key = "value" 
t[1] = 100

-- Add a print to confirm the table contents before returning
print("Table contents before return:")
print("t.key =", t.key)
print("t[1] =", t[1])

-- Explicitly confirm the type
print("Type of t:", type(t))

-- Explicit return statement
print("About to return table t")
return t -- Return the table directly