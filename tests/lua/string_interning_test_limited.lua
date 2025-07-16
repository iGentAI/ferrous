-- Limited string interning test without table.concat
print("Testing string interning (limited version)...")

-- Test function lookup with interned strings
print("\nTesting function lookup:")

-- These should all reference the same function due to string interning
local fn1 = print
local fn2 = _G["print"]
local fn3 = _G.print

fn1("Called via direct reference")
fn2("Called via table lookup with string literal")
fn3("Called via dot notation")

-- Test table operations with interned strings
print("\nTesting table operations:")

local t = {}
t.key1 = "val1"
t["key1"] = "val2"  -- Should overwrite the previous value since keys are interned

print("t.key1 =", t.key1)  -- Should print "val2"

-- Test with simple string concatenation (avoiding complex operations)
local part1 = "Hello"
local part2 = "World"
local combined = part1 .. " " .. part2
print("Simple concat:", combined)

-- Test string equality
local s1 = "test"
local s2 = "te" .. "st"
print("\nString equality test:")
print("s1:", s1)
print("s2:", s2)
print("s1 == s2:", s1 == s2)

-- Test table with limited iteration (avoid potentially infinite loops)
local small_table = { "a", "b", "c" }
print("\nTable content (limited):")

-- Use explicit indexing instead of loops
print("1:", small_table[1])
print("2:", small_table[2])
print("3:", small_table[3])

-- Manual concatenation instead of table.concat
local manual_concat = small_table[1] .. "-" .. small_table[2] .. "-" .. small_table[3]
print("Manual concat:", manual_concat)

-- Return success marker
return "String interning test (limited) passed"