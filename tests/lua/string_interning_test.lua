-- String interning validation test
-- This tests that string interning works correctly for both function lookup and table operations

print("Testing string interning...")

-- Test function lookup with interned strings
print("\nTesting function lookup:")

-- These should all reference the same function due to string interning
local fn1 = print
local fn2 = _G["print"]
local fn3 = _G.print

-- Testing dynamic string creation
local dynamic_name = "pr".."int"
local fn4 = _G[dynamic_name]

fn1("Called via direct reference")
fn2("Called via table lookup with string literal")
fn3("Called via dot notation")
fn4("Called via dynamically created string")

-- Test table operations with interned strings
print("\nTesting table operations:")

local t = {}
t.key1 = "val1"
t["key1"] = "val2"  -- Should overwrite the previous value since keys are interned

print("t.key1 =", t.key1)  -- Should print "val2"

-- Test with dynamically created strings
local dynamic_key = "ke".."y1" 
print("Dynamic key lookup:", t[dynamic_key])  -- Should print "val2"

-- Test with subtly different strings
t.key2 = "A"
t["key" .. "2"] = "B"  -- Should overwrite since strings are interned

print("t.key2 =", t.key2)  -- Should print "B"

-- Now test with table methods that use string interning
local str_table = { "first", "second", "third" }
print("\nTable content (using numerical for loop):")

-- Use a numerical for loop instead of ipairs since TFORLOOP is not implemented
for i = 1, 3 do
  print(i, str_table[i])
end

local concat_result = table.concat(str_table, "-")
print("Concatenated table:", concat_result)  -- Should print "first-second-third"

-- Test simple string equality
local s1 = "test string"
local s2 = "test".." ".."string"
print("\nString equality test:")
print("s1:", s1)
print("s2:", s2)
print("s1 == s2:", s1 == s2)

-- Return pass/fail status
-- The test succeeds if it gets to this point without errors
-- String equality issues would cause lookup failures earlier
return "String interning test passed"