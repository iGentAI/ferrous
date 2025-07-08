-- Test script for the eval opcode and function
-- This script verifies that our register window system properly
-- isolates registers during eval to avoid conflicts

-- Basic eval tests
print("== Basic eval tests ==")
local result1 = eval("return 42")
print("eval(\"return 42\") =", result1)
assert(result1 == 42, "Basic eval test failed")

local result2 = eval("return 'Hello, ' .. 'World!'")
print("eval with string concat:", result2)
assert(result2 == "Hello, World!", "String concat eval test failed")

-- Register preservation test
print("\n== Register preservation tests ==")

-- This function creates a lot of local variables to use up registers
local function register_heavy(a, b, c, d, e, f, g, h)
    -- Create some more locals to use registers
    local x1, x2, x3, x4 = 1, 2, 3, 4
    local y1, y2, y3, y4 = 5, 6, 7, 8
    
    -- Without proper register isolation, the registers used for these locals
    -- could be corrupted by the eval operation
    
    -- Run an eval that also uses a lot of registers
    local eval_result = eval([[
        -- Create lots of locals to stress register allocation
        local a, b, c, d = 10, 20, 30, 40
        local e, f, g, h = 50, 60, 70, 80
        -- Do some arithmetic to use more registers
        return a + b + c + d + e + f + g + h
    ]])
    
    print("eval_result =", eval_result)
    assert(eval_result == 360, "Eval result incorrect")
    
    -- Verify our local variables weren't corrupted
    print("Original locals after eval:", a, b, c, d, e, f, g, h)
    print("Internal locals after eval:", x1, x2, x3, x4, y1, y2, y3, y4)
    
    -- All locals should have their original values
    assert(a==1 and b==2 and c==3 and d==4 and e==5 and f==6 and g==7 and h==8, 
           "Parameter registers were corrupted")
    
    assert(x1==1 and x2==2 and x3==3 and x4==4 and y1==5 and y2==6 and y3==7 and y4==8,
           "Local registers were corrupted")
    
    return a + b + c + d + e + f + g + h
end

local sum = register_heavy(1, 2, 3, 4, 5, 6, 7, 8)
print("Sum from register_heavy:", sum)
assert(sum == 36, "Register_heavy function failed")

-- Test eval with closures
print("\n== Closures in eval tests ==")

local outer_value = 100
-- Eval creates a closure that returns a function that references local variables
local get_adder = eval([[
    local base = 10
    return function(x)
        return base + x
    end
]])

print("get_adder type:", type(get_adder))
assert(type(get_adder) == "function", "Eval should return a function")

local adder = get_adder()
print("adder type:", type(adder))
assert(type(adder) == "function", "get_adder should return a function")

local add_result = adder(5)
print("add_result =", add_result)
assert(add_result == 15, "adder should add 10 + 5")

-- Test eval with nested evals
print("\n== Nested eval tests ==")
local nested_result = eval([[
    return eval("return 21 * 2")
]])
print("nested_result =", nested_result)
assert(nested_result == 42, "Nested eval failed")

-- Test eval error handling
print("\n== Error handling tests ==")

-- Syntax error
local success, err = pcall(function()
    return eval("this is not valid lua")
end)
print("Syntax error handled:", not success)
assert(not success, "Syntax error should be caught")

-- Runtime error
local success2, err2 = pcall(function()
    return eval("return nonexistent_variable")
end)
print("Runtime error handled:", not success2)
assert(not success2, "Runtime error should be caught")

print("\nAll eval tests passed!")

-- Return a table with test results
return {
    basic_result = result1,
    string_result = result2,
    register_preservation = sum,
    closure_result = add_result,
    nested_result = nested_result,
    error_handling = not success and not success2
}