-- Minimal Equality Test
-- Tests only integer vs float equality comparison

local int_val = 1      -- integer 1
local float_val = 1.0  -- float 1.0

-- Print values directly
print("int_val =", int_val)
print("float_val =", float_val)

-- Test direct equality
print("int_val == float_val:", int_val == float_val)
print("float_val == int_val:", float_val == int_val)

-- Test equality with constants
print("int_val == 1:", int_val == 1)
print("float_val == 1.0:", float_val == 1.0)
print("1 == float_val:", 1 == float_val)
print("1.0 == int_val:", 1.0 == int_val)

-- Test in conditional context
if int_val == float_val then
    print("PASS: int_val == float_val is true in conditional")
else
    print("FAIL: int_val == float_val is false in conditional")
end

-- Test with assert
if int_val == float_val then
    print("PASS: assert would succeed")
else
    print("FAIL: assert would fail")
end

-- Simple counter function (similar to closure.lua)
function create_counter()
    local count = 0
    print("Initial count =", count)
    
    return function()
        count = count + 1
        print("New count =", count)
        print("count == 1:", count == 1)
        return count
    end
end

local counter = create_counter()
local result = counter()
print("counter() returned:", result)
print("result == 1:", result == 1)

print("Test complete")