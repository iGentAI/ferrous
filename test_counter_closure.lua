-- Counter closure test to verify upvalue capturing and state preservation
local function create_counter(initial)
    local count = initial or 0
    
    return function()
        count = count + 1
        print("Current count:", count)
        return count
    end
end

-- Create a counter with initial value 10
local counter = create_counter(10)

-- Call it multiple times to verify state preservation
print("First call:")
local result1 = counter()  -- Should return 11

print("Second call:")
local result2 = counter()  -- Should return 12

print("Third call:")
local result3 = counter()  -- Should return 13

-- Verify final result with a simple return
return result3