-- Minimal test for circular references
print("Testing minimal circular references")

-- Create a table that references itself
local t1 = {}
t1.self = t1

-- Create a cycle of two tables
local a = {}
local b = {}
a.next = b
b.back = a

-- Test self-reference
print("Self-reference table:", t1.self == t1)

-- Test cycle
print("Cycle detection:", a.next.back == a)

-- Return result
return t1.self == t1 and a.next.back == a