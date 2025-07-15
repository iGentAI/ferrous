-- Transaction safety validation test
-- This tests the transaction-based memory safety system

print("Testing transaction safety...")

-- Create tables with different lifecycles to test handle validation
local function test_table_safety()
    print("\nTesting table handle safety:")
    
    -- Create a table that will go out of scope
    local function create_and_return_table()
        local t = {}
        t.value = "inner scope"
        return t
    end
    
    -- Get the table
    local t1 = create_and_return_table()
    print("Table created and returned:", t1.value)
    
    -- Create another table
    local t2 = {}
    t2.ref = t1  -- Store reference to first table
    t2.value = "outer scope"
    
    -- Test handle validity through reference chains
    print("Chained reference:", t2.ref.value)
    
    return t2
end

-- Test string interning and handle validation 
local function test_string_safety()
    print("\nTesting string handle safety:")
    
    -- Create strings in various ways
    local s1 = "direct string"
    local s2 = "direct" .. " " .. "string"  -- Concatenated at compile time
    local s3 = "direct" .. " string"        -- Different concatenation
    
    -- Store in a table using strings as keys
    local t = {}
    t[s1] = "value1"
    t[s2] = "value2"  -- Should overwrite value1 if string interning works
    
    -- Test string equality
    print("s1 == s2:", s1 == s2)
    print("t[s1]:", t[s1])
    print("t[s2]:", t[s2])
    print("t[s3]:", t[s3])  -- Should be the same if s3 is correctly interned
    
    return t
end

-- Test function lookup with string equality
local function test_function_lookup()
    print("\nTesting function lookup:")
    
    -- Different ways to reference the same function
    local direct = print
    local global_lookup = _G["print"]
    local dynamic_name = "pr".."int"
    local dynamic_lookup = _G[dynamic_name]
    
    -- All should be the same function
    print("Functions reference test:")
    direct("Called via direct reference")
    global_lookup("Called via global table lookup")
    dynamic_lookup("Called via dynamic string lookup")
    
    -- Compare the functions for equality (implementation dependent)
    print("direct == global_lookup:", direct == global_lookup) 
    
    return direct == global_lookup and direct == dynamic_lookup
end

-- Run all tests
local t = test_table_safety()
local str_table = test_string_safety()
local func_result = test_function_lookup()

-- Return all results to validate
return t and str_table and func_result