-- Test upvalue operations

-- Create a counter with upvalues
function create_counter(initial_value)
    local count = initial_value or 0
    
    -- This function uses the upvalue 'count'
    return function(increment)
        count = count + (increment or 1)
        return count
    end
end

-- Create two counters with different initial values
local counter1 = create_counter(10)
local counter2 = create_counter(100)

-- Increment both counters and ensure they maintain separate state
local result1 = counter1(5)  -- Should be 15
local result2 = counter2(10) -- Should be 110
local result3 = counter1(2)  -- Should be 17 (maintaining state)

-- Return the results to verify
return result1, result2, result3
