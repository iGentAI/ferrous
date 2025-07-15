-- Debug version of transaction_safety_test.lua
-- Focus on just table creation and references

print("Testing transaction safety with simple nesting...")

-- Create a simple inner table that we'll reference
local t1 = {}
t1.value = "inner scope"
print("t1 created with value:", t1.value)

-- Now create an outer table that references the inner one
local t2 = {}
t2.ref = t1
t2.value = "outer scope"
print("t2 created with value:", t2.value)

-- Access the inner table through the reference
print("Accessing t1 through t2.ref...")
print("t2.ref is", type(t2.ref))
print("t2.ref.value is", t2.ref.value)

-- Return success
return (t2.ref == t1)
