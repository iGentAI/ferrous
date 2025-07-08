-- Minimal test script for register window validation

-- Simple variable assignment
local x = 10
local y = 20

-- Try a function to test register window isolation
local function add(a, b)
    return a + b
end

-- Call the function
local result = add(x, y)

-- Print the result
print("Result is:", result)

-- Return it for validation
return result