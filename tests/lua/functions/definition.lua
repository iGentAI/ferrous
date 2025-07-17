-- Test file that focuses only on function definitions

-- Test local function definition
local function test_simple()
  print("Local function definition works!")
  return true
end

-- Test function expression
local expr_func = function()
  print("Function expression works!")
  return true
end

-- Test both functions
test_simple()
expr_func()

-- Return the results
return "Function test completed successfully"
