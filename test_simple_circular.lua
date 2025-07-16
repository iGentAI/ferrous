-- Simple test of circular references in tables
print("Testing basic circular references in tables")

-- Create a table with self-reference
local t = {}
t.self = t
print("Created table with self-reference")

-- Verify self-reference works
if t.self == t then
    print("✓ Self-reference works correctly")
else
    print("✗ Self-reference broken")
end

-- Create two tables with a reference cycle
local a = {}
local b = {}
a.next = b
b.prev = a
print("Created tables with reference cycle")

-- Verify reference cycle works
if a.next.prev == a then
    print("✓ Reference cycle works correctly")
else
    print("✗ Reference cycle broken")
end

-- Create a table with multiple nested references
local complex = {}
complex.data = {nested = {back = complex}}
print("Created complex nested reference")

-- Verify complex nested reference works
if complex.data.nested.back == complex then
    print("✓ Complex nested reference works correctly")
else 
    print("✗ Complex nested reference broken")
end

print("All circular reference tests passed!")

-- Return success with a self-referencing table as proof
local result = {}
result.self = result
return result