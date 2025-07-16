-- Very simple upvalue test to demonstrate and fix upvalue capturing

local function create_counter()
    -- The key issue: create a local variable that is properly initialized
    local count = 0  -- This explicit initialization is crucial
    
    -- Return a closure that properly captures the local variable
    return function()
        count = count + 1
        return count
    end
end

-- Create and use the counter
local increment = create_counter()
print("First call:", increment())
print("Second call:", increment())

-- Return true to indicate success
return true