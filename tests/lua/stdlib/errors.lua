-- Error Handling Test
-- Tests error generation and handling in Lua
-- Status: PARTIAL - Basic error handling exists, but stack traces may be incomplete

-- Test basic error function
local function test_error()
  -- Generate an error
  error("This is a test error")
end

-- Test error with level parameter
local function inner_error()
  error("Inner error", 2)  -- Should point to the caller
end

local function outer_error()
  inner_error()
end

-- Test pcall basic functionality
local success, err = pcall(test_error)
print("pcall caught error:", not success, err)

-- Test pcall with multiple return values
local function multi_return()
  return 1, 2, 3
end

local pcall_success, a, b, c = pcall(multi_return)
print("pcall with multiple returns:", pcall_success, a, b, c)

-- Test xpcall with error handler
local function error_handler(err)
  return "Handled: " .. err
end

local xpcall_success, result = xpcall(test_error, error_handler)
print("xpcall with handler:", xpcall_success, result)

-- Test nested pcall
local function nested_errors()
  local s1, e1 = pcall(function()
    local s2, e2 = pcall(function()
      error("Deep error")
    end)
    print("Inner pcall caught:", not s2, e2)
    error("Mid error")
  end)
  print("Outer pcall caught:", not s1, e1)
  return s1, e1
end

local nested_success, nested_err = pcall(nested_errors)
print("Nested pcalls final result:", nested_success)

-- Test assert
local assert_result = assert(true, "This should not error")
print("Assert passed:", assert_result)

local assert_err_success, assert_err = pcall(function()
  assert(false, "This should error")
end)
print("Assert with false condition caught:", not assert_err_success)

-- Test catch error in coroutine 
-- (this will fail in current implementation as coroutines aren't supported)
local function test_coroutine_error()
  local co = coroutine.create(function()
    error("Coroutine error")
  end)
  
  local success, err = coroutine.resume(co)
  print("Coroutine error caught:", not success, err)
  return not success and err:match("Coroutine error")
end

local co_success, co_result = pcall(test_coroutine_error)
print("Coroutine error test:", co_success, co_result)

-- Return test results
return not success and err:match("This is a test error") and
       pcall_success and a == 1 and b == 2 and c == 3 and
       not xpcall_success and result:match("Handled:") and
       nested_success
       -- The coroutine test is expected to fail so not included in return condition