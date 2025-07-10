-- Test upvalue handling with a counter implementation

-- Create a counter with its own state
local function create_counter(initial_value)
    -- Create a local variable that will be captured as an upvalue
    local count = initial_value or 0
    
    -- Return a function that captures the count variable
    return function(increment)
        -- Update the upvalue
        count = count + (increment or 1)
        -- Return the current count
        return count
    end
end

-- Create a counter starting at 10
local counter1 = create_counter(10)

-- Call the counter with increment 5
local result = counter1(5)  -- Should be 15

-- Print the result for verification
print("Counter result:", result)

-- Call again to see if state persists
local result2 = counter1(3)  -- Should be 18
print("Counter result 2:", result2)

-- Return the final count for verification
return result2