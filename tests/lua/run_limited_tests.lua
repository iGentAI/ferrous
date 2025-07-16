-- Test runner for limited versions of problematic tests
print("Running limited test suite...")
print("=" .. string.rep("=", 40))

-- Run the simplified upvalue test
print("\n1. Running simplified upvalue test:")
local ok1, result1 = pcall(dofile, "closure_upvalue_test_simple.lua")
if ok1 then
    print("✓ Simplified upvalue test passed")
else
    print("✗ Simplified upvalue test failed:", result1)
end

-- Run the limited string interning test
print("\n2. Running limited string interning test:")
local ok2, result2 = pcall(dofile, "string_interning_test_limited.lua")
if ok2 then
    print("✓ Limited string interning test passed")
else
    print("✗ Limited string interning test failed:", result2)
end

print("\n" .. string.rep("=", 40))
print("Test Summary:")
print("Passed:", (ok1 and 1 or 0) + (ok2 and 1 or 0))
print("Failed:", (ok1 and 0 or 1) + (ok2 and 0 or 1))

-- Return overall status
return ok1 and ok2