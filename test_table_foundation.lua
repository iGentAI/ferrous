-- Test basic table operations

print("Testing table operations...")

-- Create and populate a table
local t = {}
t.key1 = "value1"
t["key2"] = "value2"
t[1] = 100
t[2] = 200

-- Print table contents
print("Table contents:")
print("  t.key1 =", t.key1)
print("  t[\"key2\"] =", t["key2"])
print("  t[1] =", t[1])
print("  t[2] =", t[2])

-- Create nested table
t.nested = {}
t.nested.x = 10
t.nested.y = 20

print("Nested table:")
print("  t.nested.x =", t.nested.x)
print("  t.nested.y =", t.nested.y)

-- Return success if everything worked
return (t.key1 == "value1" and t["key2"] == "value2" and t[1] == 100 and t.nested.x == 10)
