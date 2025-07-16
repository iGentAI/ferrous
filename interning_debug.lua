-- This script isolates string concatenation and table lookup behavior

-- First, create a simple table with a string key
local t = {}
t["print"] = "pre-interned key"

-- Print the initial state
print("Initial table setup complete...")

-- Now create the same key dynamically through concatenation
local p1 = "pr"
local p2 = "int"
local dynamic_key = p1 .. p2

print("Dynamic key created:", dynamic_key)

-- Try to access the table with the dynamic key
local value = t[dynamic_key]

-- The result should be "pre-interned key" if string interning works correctly
print("Lookup result:", value)

-- Return success or failure
if value == "pre-interned key" then
  print("SUCCESS: String interning works correctly")
  return true
else
  print("FAILURE: String interning issue detected")
  return false
end
