-- Simple closure test with minimal complexity
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

-- Return the results for verification
return {
    first_call = result1,
    second_call = result2
}