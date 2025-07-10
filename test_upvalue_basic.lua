-- Basic upvalue test

-- Create a counter with an upvalue
function create_counter()
    local count = 0
    
    -- Return a function that uses the upvalue
    return function()
        count = count + 1
        return count
    end
end

-- Create a counter
local counter = create_counter()

-- Call it three times
local result1 = counter()  -- Should be 1
local result2 = counter()  -- Should be 2
local result3 = counter()  -- Should be 3

-- Print results for verification
print("Results:", result1, result2, result3)

-- Return the final result
return result3