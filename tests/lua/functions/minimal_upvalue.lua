-- Minimal upvalue test that isolates the problem
local count = 0  -- This is the variable we'll capture

-- Simple function that just returns the upvalue (no operations)
local function get_count()
    return count
end

-- Test the upvalue access
local result = get_count()

-- Print the result
print("Count value:", result)

-- Return success marker
return "Minimal upvalue test success"