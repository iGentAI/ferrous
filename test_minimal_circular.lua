-- Minimal test for circular references in tables
print("Minimal circular reference test")

-- 1. Self-reference: create a table that references itself
local t = {}
t.self = t
print("Self-reference created correctly:", t.self == t)

-- 2. Simple cycle: create two tables that reference each other
local a = {}
local b = {}
a.ref = b
b.ref = a
print("Cycle reference created correctly:", a.ref.ref == a)

-- Return the self-referencing table
return t