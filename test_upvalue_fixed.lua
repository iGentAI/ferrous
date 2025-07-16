-- Simple upvalue test to demonstrate our fix
local function create_counter()
    -- Explicitly initialize the local variable
    local count = 0
    
    -- Return a closure that properly captures the upvalue
    return function()
        -- This should use GETUPVAL, not GETGLOBAL
        count = count + 1
        return count
    end
end

-- Create a counter and test it
local counter = create_counter()
print("First call:", counter())
print("Second call:", counter())

-- Return true to indicate success
return true