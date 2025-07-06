-- Simple closure test that doesn't return a table
local function create_counter()
    local count = 0
    return function()
        count = count + 1
        return count
    end
end

-- Create a counter
local counter = create_counter()

-- Test it by calling it twice
local result1 = counter()
local result2 = counter()

-- Print results
print("First call:", result1)
print("Second call:", result2)

-- Return a number value (not a table)
return result2